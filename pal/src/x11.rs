//! Access to the X11 server: the windows of the pal, the usable area of a
//! monitor, the pointer position, the idle time of the user, the window
//! states, and the keyboard focus.
//!
//! All values are in physical pixels of the X11 screen.

use std::cell::RefCell;
use std::collections::HashMap;
use std::os::fd::{AsFd, BorrowedFd};

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::screensaver::ConnectionExt as _;
use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ClipOrdering, ColormapAlloc, ConfigureWindowAux,
    ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, Gcontext, ImageFormat, PropMode,
    Rectangle, StackMode, VisualClass, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// Largest pixel data of one `PutImage` request, in bytes. Larger images go
/// in strips of rows.
const PUT_IMAGE_BYTES: usize = 200_000;

/// A new top-level window of the pal process.
pub struct WindowSpec<'a> {
    pub title: &'a str,
    pub rect: PxRect,
    /// The window manager does not manage the window: it has no frame, never
    /// takes the focus, and its owner must keep it on top.
    pub override_redirect: bool,
}

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PxRect {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    fn intersect(&self, o: &PxRect) -> Option<PxRect> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        (x1 > x0 && y1 > y0).then_some(PxRect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        })
    }
}

pub struct X11 {
    conn: RustConnection,
    root: Window,
    /// A 32-bit TrueColor visual with alpha, and a colormap for it.
    argb: Option<(u32, u32)>,
    /// The graphics context of each window of the pal process.
    gcs: RefCell<HashMap<Window, Gcontext>>,
    /// The error of a lost connection.
    lost: RefCell<Option<String>>,
    /// Interned atoms by name.
    atoms: RefCell<HashMap<&'static [u8], Atom>>,
    /// For each client window: the process id and the title, so that each
    /// window is queried only once.
    owners: RefCell<HashMap<Window, (Option<u32>, String)>>,
}

impl X11 {
    pub fn connect() -> Result<Self, String> {
        let (conn, screen) = x11rb::connect(None).map_err(|e| format!("no X11 display: {e}"))?;
        let s = &conn.setup().roots[screen];
        let root = s.root;
        let visual = s
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .flat_map(|d| &d.visuals)
            .find(|v| v.class == VisualClass::TRUE_COLOR)
            .map(|v| v.visual_id);
        let argb = match visual {
            Some(v) => {
                let cmap = conn.generate_id().map_err(|e| e.to_string())?;
                conn.create_colormap(ColormapAlloc::NONE, cmap, root, v)
                    .map_err(|e| e.to_string())?;
                Some((v, cmap))
            }
            None => None,
        };
        Ok(Self {
            conn,
            root,
            argb,
            gcs: RefCell::default(),
            lost: RefCell::default(),
            atoms: RefCell::default(),
            owners: RefCell::default(),
        })
    }

    /// The next event of the connection, without a wait. An error of the
    /// connection gives no event; `lost` then returns the error.
    pub fn poll_event(&self) -> Option<Event> {
        match self.conn.poll_for_event() {
            Ok(ev) => ev,
            Err(e) => {
                self.lost.borrow_mut().get_or_insert_with(|| e.to_string());
                None
            }
        }
    }

    /// The error that ended the connection to the X server, if any.
    pub fn lost(&self) -> Option<String> {
        self.lost.borrow().clone()
    }

    /// True if a compositing manager runs on the screen. Without one, the
    /// transparent parts of the windows are black.
    pub fn has_compositor(&self) -> bool {
        let Some(atom) = self.atom(b"_NET_WM_CM_S0") else {
            return false;
        };
        self.conn
            .get_selection_owner(atom)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.owner != 0)
    }

    /// Creates a window with alpha. The window is not mapped yet. A managed
    /// window has no frame, is above all other windows, is on all desktops,
    /// has no taskbar or pager entry, and does not take the keyboard focus.
    pub fn create_window(&self, spec: &WindowSpec) -> Result<Window, String> {
        let err = |e: &dyn std::fmt::Display| e.to_string();
        let (visual, cmap) = self.argb.ok_or("the X11 screen has no 32-bit visual")?;
        let win = self.conn.generate_id().map_err(|e| err(&e))?;
        let r = spec.rect;
        let aux = CreateWindowAux::new()
            .background_pixel(0)
            .border_pixel(0)
            .colormap(cmap)
            .override_redirect(u32::from(spec.override_redirect))
            .event_mask(
                EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::POINTER_MOTION
                    | EventMask::EXPOSURE
                    | EventMask::STRUCTURE_NOTIFY,
            );
        self.conn
            .create_window(
                32,
                win,
                self.root,
                r.x as i16,
                r.y as i16,
                r.w.max(1) as u16,
                r.h.max(1) as u16,
                0,
                WindowClass::INPUT_OUTPUT,
                visual,
                &aux,
            )
            .map_err(|e| err(&e))?
            .check()
            .map_err(|e| err(&e))?;
        let gc = self.conn.generate_id().map_err(|e| err(&e))?;
        self.conn
            .create_gc(gc, win, &CreateGCAux::new().graphics_exposures(0))
            .map_err(|e| err(&e))?;
        self.gcs.borrow_mut().insert(win, gc);

        let title = spec.title.as_bytes();
        let set8 = |prop: Atom, ty: Atom, data: &[u8]| {
            let _ = self
                .conn
                .change_property8(PropMode::REPLACE, win, prop, ty, data);
        };
        let set32 = |prop: Option<Atom>, ty: Atom, data: &[u32]| {
            if let Some(prop) = prop {
                let _ = self
                    .conn
                    .change_property32(PropMode::REPLACE, win, prop, ty, data);
            }
        };
        set8(AtomEnum::WM_NAME.into(), AtomEnum::STRING.into(), title);
        set8(
            AtomEnum::WM_CLASS.into(),
            AtomEnum::STRING.into(),
            b"pixelpal\0pixelpal\0",
        );
        if let (Some(name), Some(utf8)) = (self.atom(b"_NET_WM_NAME"), self.atom(b"UTF8_STRING")) {
            set8(name, utf8, title);
        }
        set32(
            self.atom(b"_NET_WM_PID"),
            AtomEnum::CARDINAL.into(),
            &[std::process::id()],
        );
        if !spec.override_redirect {
            // WM_HINTS: the input flag is set and false, so that a click on
            // the pal leaves the focus on the active window.
            set32(
                Some(AtomEnum::WM_HINTS.into()),
                AtomEnum::WM_HINTS.into(),
                &[1, 0, 0, 0, 0, 0, 0, 0, 0],
            );
            self.set_fixed_size(win, r.w, r.h);
            // _MOTIF_WM_HINTS: no decorations.
            if let Some(motif) = self.atom(b"_MOTIF_WM_HINTS") {
                set32(Some(motif), motif, &[2, 0, 0, 0, 0]);
            }
            let states: Vec<u32> = [
                b"_NET_WM_STATE_ABOVE".as_slice(),
                b"_NET_WM_STATE_STICKY",
                b"_NET_WM_STATE_SKIP_TASKBAR",
                b"_NET_WM_STATE_SKIP_PAGER",
            ]
            .into_iter()
            .filter_map(|n| self.atom(n))
            .collect();
            set32(self.atom(b"_NET_WM_STATE"), AtomEnum::ATOM.into(), &states);
        }
        let _ = self.conn.flush();
        Ok(win)
    }

    /// Sets WM_NORMAL_HINTS of `win`: the program places the window, and the
    /// size is fixed to `w` × `h`.
    fn set_fixed_size(&self, win: Window, w: i32, h: i32) {
        let (w, h) = (w.max(1) as u32, h.max(1) as u32);
        let _ = self.conn.change_property32(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            &[
                1 | 4 | 16 | 32,
                0,
                0,
                0,
                0,
                w,
                h,
                w,
                h,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ],
        );
    }

    /// Changes the size of the window `win` to `w` × `h`.
    pub fn resize_window(&self, win: Window, w: i32, h: i32) {
        self.set_fixed_size(win, w, h);
        let aux = ConfigureWindowAux::new()
            .width(w.max(1) as u32)
            .height(h.max(1) as u32);
        let _ = self.conn.configure_window(win, &aux);
        let _ = self.conn.flush();
    }

    /// Shows or hides the window `win`.
    pub fn set_mapped(&self, win: Window, mapped: bool) {
        let _ = if mapped {
            self.conn.map_window(win)
        } else {
            self.conn.unmap_window(win)
        };
        let _ = self.conn.flush();
    }

    /// Moves the window `win` to `(x, y)`.
    pub fn move_window(&self, win: Window, x: i32, y: i32) {
        let _ = self
            .conn
            .configure_window(win, &ConfigureWindowAux::new().x(x).y(y));
        let _ = self.conn.flush();
    }

    /// Raises the window `win` above all other windows. The window manager
    /// does not stack windows that it does not manage, so their owner must
    /// raise them.
    pub fn raise(&self, win: Window) {
        let aux = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
        let _ = self.conn.configure_window(win, &aux);
        let _ = self.conn.flush();
    }

    /// The position of the top-left corner of `win` on the screen.
    pub fn window_origin(&self, win: Window) -> Option<(i32, i32)> {
        let r = self
            .conn
            .translate_coordinates(win, self.root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        Some((r.dst_x.into(), r.dst_y.into()))
    }

    /// Copies the premultiplied RGBA pixels `rgba`, `width` pixels per row,
    /// into the window `win`.
    pub fn put_pixels(&self, win: Window, width: u32, rgba: &[u8]) {
        let Some(&gc) = self.gcs.borrow().get(&win) else {
            return;
        };
        let row = width as usize * 4;
        if row == 0 {
            return;
        }
        // The 32-bit visual takes the bytes B, G, R, A of each pixel.
        let mut bgra = rgba.to_vec();
        for p in bgra.chunks_exact_mut(4) {
            p.swap(0, 2);
        }
        let rows = (PUT_IMAGE_BYTES / row).max(1);
        for (i, strip) in bgra.chunks(rows * row).enumerate() {
            let _ = self.conn.put_image(
                ImageFormat::Z_PIXMAP,
                win,
                gc,
                width as u16,
                (strip.len() / row) as u16,
                0,
                (i * rows) as i16,
                0,
                32,
                strip,
            );
        }
        let _ = self.conn.flush();
    }

    /// Lets all clicks on `win` through to the windows below.
    pub fn clear_input(&self, win: Window) {
        let _ =
            self.conn
                .shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, win, 0, 0, &[]);
        let _ = self.conn.flush();
    }

    fn atom(&self, name: &'static [u8]) -> Option<Atom> {
        if let Some(&atom) = self.atoms.borrow().get(name) {
            return Some(atom);
        }
        let atom = self.conn.intern_atom(false, name).ok()?.reply().ok()?.atom;
        (atom != 0).then(|| {
            self.atoms.borrow_mut().insert(name, atom);
            atom
        })
    }

    fn prop32(&self, win: Window, prop: Atom, ty: impl Into<Atom>) -> Vec<u32> {
        self.conn
            .get_property(false, win, prop, ty, 0, 1024)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    /// The client windows of the process `pid`, with their titles. A window
    /// that appears for the first time is queried; known windows come from a
    /// cache.
    fn windows_of(&self, pid: u32) -> Vec<(Window, String)> {
        let (Some(client_list), Some(wm_pid), Some(wm_name), Some(utf8)) = (
            self.atom(b"_NET_CLIENT_LIST"),
            self.atom(b"_NET_WM_PID"),
            self.atom(b"_NET_WM_NAME"),
            self.atom(b"UTF8_STRING"),
        ) else {
            return Vec::new();
        };
        let clients = self.prop32(self.root, client_list, AtomEnum::WINDOW);
        let mut owners = self.owners.borrow_mut();
        owners.retain(|w, _| clients.contains(w));
        let mut result = Vec::new();
        for win in clients {
            let (owner, title) = owners.entry(win).or_insert_with(|| {
                let owner = self
                    .prop32(win, wm_pid, AtomEnum::CARDINAL)
                    .first()
                    .copied();
                let title = if owner == Some(pid) {
                    self.conn
                        .get_property(false, win, wm_name, utf8, 0, 256)
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .map(|r| String::from_utf8_lossy(&r.value).into_owned())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                (owner, title)
            });
            if *owner == Some(pid) {
                result.push((win, title.clone()));
            }
        }
        result
    }

    /// Sets the window states of the window `win` again: above all other
    /// windows, on all desktops, and without a taskbar or pager entry. The
    /// window manager can remove these states, so call this function
    /// periodically.
    pub fn keep_states(&self, win: Window) {
        let atoms = [
            b"_NET_WM_STATE".as_slice(),
            b"_NET_WM_STATE_ABOVE",
            b"_NET_WM_STATE_STICKY",
            b"_NET_WM_STATE_SKIP_TASKBAR",
            b"_NET_WM_STATE_SKIP_PAGER",
        ]
        .map(|n| self.atom(n));
        let [
            Some(state),
            Some(above),
            Some(sticky),
            Some(skip_taskbar),
            Some(skip_pager),
        ] = atoms
        else {
            return;
        };
        let wanted = [skip_taskbar, skip_pager, above, sticky];
        let current = self.prop32(win, state, AtomEnum::ATOM);
        let missing: Vec<u32> = wanted
            .into_iter()
            .filter(|a| !current.contains(a))
            .collect();
        // One _NET_WM_STATE request adds up to two states.
        // Action 1 = add, source 1 = application.
        for pair in missing.chunks(2) {
            let second = pair.get(1).copied().unwrap_or(0);
            let event = ClientMessageEvent::new(32, win, state, [1, pair[0], second, 1, 0]);
            let _ = self.conn.send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            );
        }
        let _ = self.conn.flush();
    }

    /// The window of the process `pid` with the title `title`.
    pub fn find_window(&self, pid: u32, title: &str) -> Option<Window> {
        self.windows_of(pid)
            .into_iter()
            .find(|(_, t)| t == title)
            .map(|(w, _)| w)
    }

    /// The active window (`_NET_ACTIVE_WINDOW`).
    pub fn active_window(&self) -> Option<Window> {
        let active = self.atom(b"_NET_ACTIVE_WINDOW")?;
        self.conn
            .get_property(false, self.root, active, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .next()
    }

    /// Asks the window manager to activate `win` and to give it the keyboard focus.
    pub fn activate(&self, win: Window) {
        let Some(active) = self.atom(b"_NET_ACTIVE_WINDOW") else {
            return;
        };
        // Source 2 = pager: window managers apply such requests without focus
        // stealing prevention.
        let event = ClientMessageEvent::new(32, win, active, [2, 0, 0, 0, 0]);
        let _ = self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        );
        let _ = self.conn.flush();
    }

    /// The usable area of the monitor that contains the point `(x, y)`.
    ///
    /// This is the monitor rectangle (RandR) intersected with `_NET_WORKAREA`, so
    /// that the area excludes the desktop panels.
    pub fn work_area_at(&self, x: i32, y: i32) -> Option<PxRect> {
        let monitors = self.monitors();
        let monitor = monitors
            .iter()
            .find(|m| m.contains(x, y))
            .or(monitors.first())
            .copied()
            .or_else(|| self.root_rect())?;
        match self.net_workarea() {
            Some(wa) => Some(monitor.intersect(&wa).unwrap_or(monitor)),
            None => Some(monitor),
        }
    }

    /// The global position of the mouse pointer.
    pub fn pointer(&self) -> Option<(i32, i32)> {
        let reply = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        Some((reply.root_x as i32, reply.root_y as i32))
    }

    /// Limits the input region of `win` to `rect`, in window coordinates:
    /// clicks outside of it go to the windows below.
    pub fn set_input_rect(&self, win: Window, rect: PxRect) {
        let r = Rectangle {
            x: rect.x as i16,
            y: rect.y as i16,
            width: rect.w.max(1) as u16,
            height: rect.h.max(1) as u16,
        };
        let _ =
            self.conn
                .shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, win, 0, 0, &[r]);
        let _ = self.conn.flush();
    }

    /// Time since the last keyboard or mouse input, from the XScreenSaver
    /// extension. `None` if the server lacks the extension.
    pub fn idle_time(&self) -> Option<std::time::Duration> {
        let reply = self
            .conn
            .screensaver_query_info(self.root)
            .ok()?
            .reply()
            .ok()?;
        Some(std::time::Duration::from_millis(
            reply.ms_since_user_input as u64,
        ))
    }

    fn monitors(&self) -> Vec<PxRect> {
        let Some(reply) = self
            .conn
            .randr_get_monitors(self.root, true)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return Vec::new();
        };
        reply
            .monitors
            .iter()
            .map(|m| PxRect {
                x: m.x as i32,
                y: m.y as i32,
                w: m.width as i32,
                h: m.height as i32,
            })
            .collect()
    }

    fn root_rect(&self) -> Option<PxRect> {
        let g = self.conn.get_geometry(self.root).ok()?.reply().ok()?;
        Some(PxRect {
            x: 0,
            y: 0,
            w: g.width as i32,
            h: g.height as i32,
        })
    }

    /// The rectangle of `_NET_WORKAREA` for the current desktop
    /// (`_NET_CURRENT_DESKTOP`, or desktop 0 if the property is missing).
    fn net_workarea(&self) -> Option<PxRect> {
        let atom = self.atom(b"_NET_WORKAREA")?;
        let desktop = self
            .atom(b"_NET_CURRENT_DESKTOP")
            .and_then(|a| {
                self.conn
                    .get_property(false, self.root, a, AtomEnum::CARDINAL, 0, 1)
                    .ok()?
                    .reply()
                    .ok()?
                    .value32()?
                    .next()
            })
            .unwrap_or(0);
        let reply = self
            .conn
            .get_property(false, self.root, atom, AtomEnum::CARDINAL, desktop * 4, 4)
            .ok()?
            .reply()
            .ok()?;
        let v: Vec<u32> = reply.value32()?.collect();
        (v.len() == 4).then(|| PxRect {
            x: v[0] as i32,
            y: v[1] as i32,
            w: v[2] as i32,
            h: v[3] as i32,
        })
    }
}

impl AsFd for X11 {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.conn.stream().as_fd()
    }
}
