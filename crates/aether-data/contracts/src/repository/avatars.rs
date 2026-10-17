//! Profile pictures. One small image per user, stored by the gateway itself so that nobody's
//! browser has to fetch pictures from third-party hosts.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::DataLayerError;

/// Largest accepted image, after the console re-encoded it to a small square (usually ~10-30 KiB).
pub const MAX_AVATAR_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAvatar {
    pub content_type: String,
    pub data: Vec<u8>,
    /// Unix milliseconds of the last change; doubles as the cache-busting version.
    pub version: i64,
}

/// The image formats accepted for avatars. SVG is deliberately absent: it can carry scripts.
pub fn avatar_content_type_for(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

#[async_trait]
pub trait UserAvatarRepository: Send + Sync + std::fmt::Debug {
    /// Replaces the user's picture and returns its new version.
    async fn put(
        &self,
        user_id: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<i64, DataLayerError>;
    async fn get(&self, user_id: &str) -> Result<Option<StoredAvatar>, DataLayerError>;
    /// Returns whether there was a picture to remove.
    async fn delete(&self, user_id: &str) -> Result<bool, DataLayerError>;
    /// Versions of the users that have a picture; users without one are absent.
    async fn versions(&self, user_ids: &[String]) -> Result<HashMap<String, i64>, DataLayerError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_png_jpeg_and_webp_are_recognised_by_their_bytes() {
        assert_eq!(
            avatar_content_type_for(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]),
            Some("image/png")
        );
        assert_eq!(
            avatar_content_type_for(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some("image/jpeg")
        );
        assert_eq!(
            avatar_content_type_for(b"RIFF\x10\0\0\0WEBPVP8 "),
            Some("image/webp")
        );
        for rejected in [
            &b"<svg xmlns='http://www.w3.org/2000/svg'><script/></svg>"[..],
            b"GIF89a",
            b"RIFF\x10\0\0\0WAVEfmt ",
            b"",
            b"not an image",
        ] {
            assert_eq!(avatar_content_type_for(rejected), None, "{rejected:?}");
        }
    }
}
