//! 感知哈希模块
//!
//! 用于计算图像的感知哈希，判断帧之间的相似度。

/// 感知哈希计算器
pub struct PerceptualHasher;

impl PerceptualHasher {
    /// 创建新的哈希计算器
    pub fn new() -> Self {
        Self
    }

    /// 计算 RGBA 图像的感知哈希（差值哈希 dHash）
    ///
    /// 返回 8 字节（64 位）哈希值
    pub fn compute(&self, pixels: &[u8], width: u32, height: u32) -> [u8; 8] {
        // 1. 缩小到 9x8（为了计算 8x8 的差值）
        let small = self.resize_grayscale(pixels, width, height, 9, 8);

        // 2. 计算差值哈希
        let mut hash = [0u8; 8];
        let mut bit_index = 0;

        for y in 0..8 {
            for x in 0..8 {
                let left = small[y * 9 + x];
                let right = small[y * 9 + x + 1];

                if left > right {
                    let byte_index = bit_index / 8;
                    let bit_offset = 7 - (bit_index % 8);
                    hash[byte_index] |= 1 << bit_offset;
                }
                bit_index += 1;
            }
        }

        hash
    }

    /// 计算两个哈希之间的汉明距离
    pub fn hamming_distance(&self, hash1: &[u8; 8], hash2: &[u8; 8]) -> u32 {
        let mut distance = 0u32;
        for i in 0..8 {
            distance += (hash1[i] ^ hash2[i]).count_ones();
        }
        distance
    }

    /// 缩放并转换为灰度图
    fn resize_grayscale(
        &self,
        pixels: &[u8],
        src_width: u32,
        src_height: u32,
        dst_width: usize,
        dst_height: usize,
    ) -> Vec<u8> {
        let mut result = vec![0u8; dst_width * dst_height];

        let x_ratio = src_width as f64 / dst_width as f64;
        let y_ratio = src_height as f64 / dst_height as f64;

        for y in 0..dst_height {
            for x in 0..dst_width {
                let src_x = (x as f64 * x_ratio) as u32;
                let src_y = (y as f64 * y_ratio) as u32;

                let idx = ((src_y * src_width + src_x) * 4) as usize;
                if idx + 2 < pixels.len() {
                    // RGBA -> 灰度 (简化公式)
                    let r = pixels[idx] as u32;
                    let g = pixels[idx + 1] as u32;
                    let b = pixels[idx + 2] as u32;
                    let gray = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
                    result[y * dst_width + x] = gray;
                }
            }
        }

        result
    }
}

impl Default for PerceptualHasher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_images_have_zero_distance() {
        let hasher = PerceptualHasher::new();
        let pixels = vec![128u8; 100 * 100 * 4]; // 灰色图像

        let hash1 = hasher.compute(&pixels, 100, 100);
        let hash2 = hasher.compute(&pixels, 100, 100);

        assert_eq!(hasher.hamming_distance(&hash1, &hash2), 0);
    }

    #[test]
    fn test_different_images_have_nonzero_distance() {
        let hasher = PerceptualHasher::new();

        // 白色图像
        let white = vec![255u8; 100 * 100 * 4];
        // 黑色图像
        let black = vec![0u8; 100 * 100 * 4];

        let hash1 = hasher.compute(&white, 100, 100);
        let hash2 = hasher.compute(&black, 100, 100);

        // 差异应该很大
        assert!(hasher.hamming_distance(&hash1, &hash2) > 0);
    }
}
