//! 屏幕捕获模块

use anyhow::Result;
use chrono::Utc;
use tracing::debug;

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
}

impl ScreenCapture {
    /// 创建新的屏幕捕获器
    pub fn new() -> Result<Self> {
        Ok(Self {
            target_width: 1920,
            target_height: 1080,
        })
    }

    /// 捕获当前屏幕
    pub fn capture(&mut self) -> Result<CapturedFrame> {
        let timestamp = Utc::now().timestamp_millis();

        // 使用 xcap 捕获屏幕
        let monitors = xcap::Monitor::all()?;
        let monitor = monitors
            .first()
            .ok_or_else(|| anyhow::anyhow!("No monitor found"))?;

        let image = monitor.capture_image()?;
        let (width, height) = (image.width(), image.height());

        debug!("Captured screen: {}x{}", width, height);

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
