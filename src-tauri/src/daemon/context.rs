//! 窗口上下文监控模块

use tracing::debug;

/// 焦点窗口上下文
#[derive(Debug, Clone, Default)]
pub struct FocusContext {
    /// 应用名称
    pub app_name: Option<String>,
    /// 窗口标题
    pub window_title: Option<String>,
    /// 是否全屏
    pub is_fullscreen: bool,
    /// 窗口位置 (x, y, width, height)
    pub bounds: Option<(i32, i32, u32, u32)>,
    /// 进程 ID
    pub pid: Option<u32>,
}

/// 窗口监控器
pub struct WindowWatcher;

impl WindowWatcher {
    /// 获取当前焦点窗口上下文
    pub fn get_focus_context() -> FocusContext {
        #[cfg(target_os = "linux")]
        {
            Self::get_linux_focus_context()
        }

        #[cfg(target_os = "windows")]
        {
            Self::get_windows_focus_context()
        }

        #[cfg(target_os = "macos")]
        {
            Self::get_macos_focus_context()
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            FocusContext::default()
        }
    }

    #[cfg(target_os = "linux")]
    fn get_linux_focus_context() -> FocusContext {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::{
            AtomEnum, ConnectionExt, GetPropertyReply, Window,
        };

        debug!("Getting Linux focus context via X11");

        // 尝试连接到 X11 服务器
        let conn = match x11rb::connect(None) {
            Ok((conn, _)) => conn,
            Err(e) => {
                debug!("Failed to connect to X11: {}", e);
                return FocusContext::default();
            }
        };

        // 获取根窗口
        let screen = conn.setup().roots.first();
        let root = match screen {
            Some(s) => s.root,
            None => {
                debug!("No X11 screen found");
                return FocusContext::default();
            }
        };

        // 获取 _NET_ACTIVE_WINDOW atom
        let active_window_atom = match conn.intern_atom(false, b"_NET_ACTIVE_WINDOW") {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => reply.atom,
                Err(_) => return FocusContext::default(),
            },
            Err(_) => return FocusContext::default(),
        };

        // 获取 _NET_WM_NAME atom
        let wm_name_atom = match conn.intern_atom(false, b"_NET_WM_NAME") {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => reply.atom,
                Err(_) => return FocusContext::default(),
            },
            Err(_) => return FocusContext::default(),
        };

        // 获取 UTF8_STRING atom
        let utf8_string_atom = match conn.intern_atom(false, b"UTF8_STRING") {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => reply.atom,
                Err(_) => return FocusContext::default(),
            },
            Err(_) => return FocusContext::default(),
        };

        // 获取 _NET_WM_PID atom
        let wm_pid_atom = match conn.intern_atom(false, b"_NET_WM_PID") {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => reply.atom,
                Err(_) => return FocusContext::default(),
            },
            Err(_) => return FocusContext::default(),
        };

        // 获取 WM_CLASS atom
        let wm_class_atom = match conn.intern_atom(false, b"WM_CLASS") {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => reply.atom,
                Err(_) => return FocusContext::default(),
            },
            Err(_) => return FocusContext::default(),
        };

        // 获取当前活动窗口
        let active_window: Window = match conn.get_property(false, root, active_window_atom, AtomEnum::WINDOW, 0, 1) {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => {
                    if reply.value.len() >= 4 {
                        u32::from_ne_bytes([
                            reply.value[0],
                            reply.value[1],
                            reply.value[2],
                            reply.value[3],
                        ])
                    } else {
                        return FocusContext::default();
                    }
                }
                Err(_) => return FocusContext::default(),
            },
            Err(_) => return FocusContext::default(),
        };

        if active_window == 0 {
            return FocusContext::default();
        }

        // 获取窗口标题 (_NET_WM_NAME)
        let window_title = Self::get_x11_text_property(&conn, active_window, wm_name_atom, utf8_string_atom)
            .or_else(|| {
                // 回退到 WM_NAME
                let wm_name_legacy = conn.intern_atom(false, b"WM_NAME")
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .map(|r| r.atom);
                wm_name_legacy.and_then(|atom| Self::get_x11_text_property(&conn, active_window, atom, AtomEnum::STRING.into()))
            });

        // 获取应用名称 (WM_CLASS)
        let app_name = Self::get_wm_class(&conn, active_window, wm_class_atom);

        // 获取 PID
        let pid = Self::get_x11_cardinal_property(&conn, active_window, wm_pid_atom);

        // 获取窗口几何信息
        let bounds = match conn.get_geometry(active_window) {
            Ok(cookie) => match cookie.reply() {
                Ok(geom) => Some((
                    geom.x as i32,
                    geom.y as i32,
                    geom.width as u32,
                    geom.height as u32,
                )),
                Err(_) => None,
            },
            Err(_) => None,
        };

        // 检测是否全屏
        let is_fullscreen = Self::check_fullscreen(&conn, active_window, root);

        debug!(
            "Focus context: app={:?}, title={:?}, pid={:?}, bounds={:?}, fullscreen={}",
            app_name, window_title, pid, bounds, is_fullscreen
        );

        FocusContext {
            app_name,
            window_title,
            is_fullscreen,
            bounds,
            pid,
        }
    }

    #[cfg(target_os = "linux")]
    fn get_x11_text_property<C: x11rb::connection::Connection>(
        conn: &C,
        window: x11rb::protocol::xproto::Window,
        property: x11rb::protocol::xproto::Atom,
        expected_type: x11rb::protocol::xproto::Atom,
    ) -> Option<String> {
        use x11rb::protocol::xproto::ConnectionExt;

        conn.get_property(false, window, property, expected_type, 0, 1024)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| {
                if !reply.value.is_empty() {
                    String::from_utf8(reply.value).ok()
                } else {
                    None
                }
            })
    }

    #[cfg(target_os = "linux")]
    fn get_wm_class<C: x11rb::connection::Connection>(
        conn: &C,
        window: x11rb::protocol::xproto::Window,
        wm_class_atom: x11rb::protocol::xproto::Atom,
    ) -> Option<String> {
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

        conn.get_property(false, window, wm_class_atom, AtomEnum::STRING, 0, 1024)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| {
                // WM_CLASS 包含两个 null 分隔的字符串: instance 和 class
                // 我们使用 class (第二个字符串)
                let parts: Vec<&[u8]> = reply.value.split(|&b| b == 0).collect();
                if parts.len() >= 2 && !parts[1].is_empty() {
                    String::from_utf8(parts[1].to_vec()).ok()
                } else if !parts.is_empty() && !parts[0].is_empty() {
                    String::from_utf8(parts[0].to_vec()).ok()
                } else {
                    None
                }
            })
    }

    #[cfg(target_os = "linux")]
    fn get_x11_cardinal_property<C: x11rb::connection::Connection>(
        conn: &C,
        window: x11rb::protocol::xproto::Window,
        property: x11rb::protocol::xproto::Atom,
    ) -> Option<u32> {
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

        conn.get_property(false, window, property, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| {
                if reply.value.len() >= 4 {
                    Some(u32::from_ne_bytes([
                        reply.value[0],
                        reply.value[1],
                        reply.value[2],
                        reply.value[3],
                    ]))
                } else {
                    None
                }
            })
    }

    #[cfg(target_os = "linux")]
    fn check_fullscreen<C: x11rb::connection::Connection>(
        conn: &C,
        window: x11rb::protocol::xproto::Window,
        root: x11rb::protocol::xproto::Window,
    ) -> bool {
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

        // 获取 _NET_WM_STATE 和 _NET_WM_STATE_FULLSCREEN atoms
        let wm_state_atom = conn.intern_atom(false, b"_NET_WM_STATE")
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.atom);

        let fullscreen_atom = conn.intern_atom(false, b"_NET_WM_STATE_FULLSCREEN")
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.atom);

        if let (Some(state_atom), Some(fs_atom)) = (wm_state_atom, fullscreen_atom) {
            if let Ok(cookie) = conn.get_property(false, window, state_atom, AtomEnum::ATOM, 0, 32) {
                if let Ok(reply) = cookie.reply() {
                    // 检查 _NET_WM_STATE_FULLSCREEN 是否在状态列表中
                    let atoms: Vec<u32> = reply.value
                        .chunks_exact(4)
                        .map(|chunk| u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                        .collect();
                    return atoms.contains(&fs_atom);
                }
            }
        }

        // 回退: 检查窗口大小是否匹配屏幕大小
        if let (Ok(geom), Ok(screen_cookie)) = (
            conn.get_geometry(window).and_then(|c| c.reply()),
            conn.get_geometry(root).and_then(|c| c.reply()),
        ) {
            return geom.width == screen_cookie.width && geom.height == screen_cookie.height;
        }

        false
    }

    #[cfg(target_os = "windows")]
    fn get_windows_focus_context() -> FocusContext {
        // TODO: 使用 Windows API 获取窗口信息
        debug!("Getting Windows focus context (not implemented)");
        FocusContext::default()
    }

    #[cfg(target_os = "macos")]
    fn get_macos_focus_context() -> FocusContext {
        // TODO: 使用 macOS API 获取窗口信息
        debug!("Getting macOS focus context (not implemented)");
        FocusContext::default()
    }
}
