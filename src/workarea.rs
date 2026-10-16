//! Access to the X11 server: the usable area of a monitor, the pointer position,
//! the idle time of the user,
//! and the stacking of a window on top of all other windows.
//!
//! All values are in physical pixels of the X11 screen.

use std::cell::RefCell;
use std::collections::HashMap;

use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::screensaver::ConnectionExt as _;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, Window,
};
use x11rb::rust_connection::RustConnection;

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
    /// Interned atoms by name.
    atoms: RefCell<HashMap<&'static [u8], Atom>>,
    /// For each client window: the process id and the title, so that each
    /// window is queried only once.
    owners: RefCell<HashMap<Window, (Option<u32>, String)>>,
}

impl X11 {
    pub fn connect() -> Option<Self> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots[screen].root;
        Some(Self {
            conn,
            root,
            atoms: RefCell::default(),
            owners: RefCell::default(),
        })
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

    /// Sets the window states of the windows of the process `pid`.
    ///
    /// The windows with a title in `titles` are above all other windows, on all
    /// desktops, and have no taskbar or pager entry. The other windows keep their
    /// states: the window manager does not activate a window that skips the
    /// taskbar and the pager, and the chat panel needs the keyboard focus. The
    /// window manager can remove these states, so call this function
    /// periodically.
    pub fn manage_windows(&self, pid: u32, titles: &[&str]) {
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
        for (win, title) in self.windows_of(pid) {
            if !titles.contains(&title.as_str()) {
                continue;
            }
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
        use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
        use x11rb::protocol::xproto::{ClipOrdering, Rectangle};
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
