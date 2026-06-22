/// Map a MIME type to a primary asset kind for browse/UI.
pub fn kind_from_mime(mime: &str) -> &'static str {
    let mime = mime.trim().to_ascii_lowercase();
    if mime.starts_with("image/") {
        "image"
    } else if mime.starts_with("video/") {
        "video"
    } else if mime.starts_with("audio/") {
        "audio"
    } else {
        "file"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_media_mimes() {
        assert_eq!(kind_from_mime("image/jpeg"), "image");
        assert_eq!(kind_from_mime("video/mp4"), "video");
        assert_eq!(kind_from_mime("audio/mpeg"), "audio");
        assert_eq!(kind_from_mime("application/pdf"), "file");
    }
}
