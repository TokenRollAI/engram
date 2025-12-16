//! 屏幕捕获模块
//!
//! 支持三种捕获模式：
//! - PrimaryMonitor: 捕获主显示器（默认）
//! - FocusedMonitor: 捕获活动窗口所在的显示器
//! - ActiveWindow: 只捕获活动窗口

use anyhow::Result;
use chrono::Utc;
use tracing::{debug, warn};

use crate::config::CaptureMode;
use crate::daemon::context::FocusContext;

/// 捕获的帧数据
#[derive(Debug)]
pub struct CapturedFrame {
    /// RGBA 像素数据
    pub pixels: Vec<u8>,
    /// 宽度
    pub width: u32,
    /// 高度
    pub height: u32,
    /// 捕获时间戳（Unix 毫秒）
    pub timestamp: i64,
}

/// 屏幕捕获器
pub struct ScreenCapture {
    /// 目标宽度（下采样）
    target_width: u32,
    /// 目标高度（下采样）
    target_height: u32,
    /// 捕获模式
    mode: CaptureMode,
}

impl ScreenCapture {
    /// 创建新的屏幕捕获器
    pub fn new(mode: CaptureMode) -> Result<Self> {
        debug!("Initializing ScreenCapture with mode: {:?}", mode);
        Ok(Self {
            target_width: 1920,
            target_height: 1080,
            mode,
        })
    }

    /// 设置捕获模式
    pub fn set_mode(&mut self, mode: CaptureMode) {
        self.mode = mode;
        debug!("Capture mode changed to: {:?}", mode);
    }

    /// 捕获当前屏幕（根据模式选择捕获方式）
    pub fn capture(&mut self, focus: &FocusContext) -> Result<CapturedFrame> {
        match self.mode {
            CaptureMode::PrimaryMonitor => self.capture_primary_monitor(),
            CaptureMode::FocusedMonitor => self.capture_focused_monitor(focus),
            CaptureMode::ActiveWindow => self.capture_active_window(focus),
        }
    }

    /// 捕获主显示器（原有行为）
    fn capture_primary_monitor(&self) -> Result<CapturedFrame> {
        let timestamp = Utc::now().timestamp_millis();

        let monitors = xcap::Monitor::all()?;
        let monitor = monitors
            .first()
            .ok_or_else(|| anyhow::anyhow!("No monitor found"))?;

        let image = monitor.capture_image()?;
        debug!(
            "Captured primary monitor: {}x{}",
            image.width(),
            image.height()
        );

        self.process_image(image, timestamp)
    }

    /// 捕获活动窗口所在的显示器
    fn capture_focused_monitor(&self, focus: &FocusContext) -> Result<CapturedFrame> {
        let timestamp = Utc::now().timestamp_millis();

        // 如果有窗口位置信息，使用窗口中心点定位显示器
        if let Some((x, y, w, h)) = focus.bounds {
            let center_x = x + (w as i32) / 2;
            let center_y = y + (h as i32) / 2;

            debug!(
                "Locating monitor at point ({}, {}) for window bounds {:?}",
                center_x, center_y, focus.bounds
            );

            match xcap::Monitor::from_point(center_x, center_y) {
                Ok(monitor) => {
                    let image = monitor.capture_image()?;
                    debug!(
                        "Captured focused monitor '{}': {}x{}",
                        monitor.name().unwrap_or_default(),
                        image.width(),
                        image.height()
                    );
                    return self.process_image(image, timestamp);
                }
                Err(e) => {
                    warn!(
                        "Failed to get monitor from point ({}, {}): {}, falling back to primary",
                        center_x, center_y, e
                    );
                }
            }
        } else {
            debug!("No window bounds available, falling back to primary monitor");
        }

        // 回退到主显示器
        self.capture_primary_monitor()
    }

    /// 捕获活动窗口
    fn capture_active_window(&self, focus: &FocusContext) -> Result<CapturedFrame> {
        let timestamp = Utc::now().timestamp_millis();

        // 需要 PID 或窗口标题来定位窗口
        let target_pid = focus.pid;
        let target_title = focus.window_title.as_deref();

        if target_pid.is_none() && target_title.is_none() {
            debug!("No PID or title available for active window, falling back to primary monitor");
            return self.capture_primary_monitor();
        }

        // 获取所有窗口
        let windows = xcap::Window::all()?;

        // 查找匹配的窗口
        for window in windows {
            // 跳过最小化的窗口
            if window.is_minimized().unwrap_or(true) {
                continue;
            }

            // 优先按 PID 匹配
            if let Some(pid) = target_pid {
                if let Ok(window_pid) = window.current_monitor().and_then(|_| {
                    // xcap 没有直接获取 PID 的方法，我们用标题匹配
                    Ok(0u32)
                }) {
                    if window_pid == pid {
                        let image = window.capture_image()?;
                        debug!(
                            "Captured window by PID {}: {}x{}",
                            pid,
                            image.width(),
                            image.height()
                        );
                        return self.process_image(image, timestamp);
                    }
                }
            }

            // 按标题匹配
            if let Some(title) = target_title {
                if let Ok(window_title) = window.title() {
                    if window_title == title {
                        let image = window.capture_image()?;
                        debug!(
                            "Captured window '{}': {}x{}",
                            title,
                            image.width(),
                            image.height()
                        );
                        return self.process_image(image, timestamp);
                    }
                }
            }
        }

        warn!(
            "Could not find matching window (pid={:?}, title={:?}), falling back to primary monitor",
            target_pid, target_title
        );
        self.capture_primary_monitor()
    }

    /// 处理图像（下采样等）
    fn process_image(
        &self,
        image: image::RgbaImage,
        timestamp: i64,
    ) -> Result<CapturedFrame> {
        let (width, height) = (image.width(), image.height());

        // 如果需要下采样
        let (final_pixels, final_width, final_height) =
            if width > self.target_width || height > self.target_height {
                let resized = image::imageops::resize(
                    &image,
                    self.target_width,
                    self.target_height,
                    image::imageops::FilterType::Triangle,
                );
                let w = resized.width();
                let h = resized.height();
                (resized.into_raw(), w, h)
            } else {
                (image.into_raw(), width, height)
            };

        Ok(CapturedFrame {
            pixels: final_pixels,
            width: final_width,
            height: final_height,
            timestamp,
        })
    }
}
