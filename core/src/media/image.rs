use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use archiveos_contract::VaultError;
use chrono::{NaiveDateTime, TimeZone, Utc};
use exif::{In, Reader, Tag, Value};
use rusqlite::Connection;
use uuid::Uuid;

use crate::import::db;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExtractedImageMetadata {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub taken_at: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub gps_latitude: Option<String>,
    pub gps_longitude: Option<String>,
}

pub fn is_image_path(path: &Path) -> bool {
    mime_guess::from_path(path)
        .first_raw()
        .is_some_and(|mime| mime.starts_with("image/"))
}

pub fn extract(path: &Path) -> ExtractedImageMetadata {
    if !is_image_path(path) {
        return ExtractedImageMetadata::default();
    }

    let mut meta = ExtractedImageMetadata::default();
    if let Ok((width, height)) = image::image_dimensions(path) {
        meta.width = Some(width);
        meta.height = Some(height);
    }

    if let Ok(file) = File::open(path) {
        let mut reader = BufReader::new(file);
        let exif = Reader::new().read_from_container(&mut reader);
        if let Ok(exif) = exif {
            meta.taken_at = exif_datetime(&exif, Tag::DateTimeOriginal)
                .or_else(|| exif_datetime(&exif, Tag::DateTimeDigitized))
                .or_else(|| exif_datetime(&exif, Tag::DateTime));
            meta.camera_make = exif_ascii(&exif, Tag::Make);
            meta.camera_model = exif_ascii(&exif, Tag::Model);
            if let Some((lat, lon)) = exif_gps(&exif) {
                meta.gps_latitude = Some(format!("{lat:.6}"));
                meta.gps_longitude = Some(format!("{lon:.6}"));
            }
        }
    }

    meta
}

pub fn persist(
    conn: &Connection,
    entity_id: Uuid,
    extracted: &ExtractedImageMetadata,
    fs_modified_at: Option<&str>,
) -> Result<(), VaultError> {
    if extracted.is_empty() && fs_modified_at.is_none() {
        return Ok(());
    }

    if let Some(taken_at) = &extracted.taken_at {
        db::update_entity_created_at(conn, entity_id, taken_at)?;
        db::upsert_metadata(conn, entity_id, "taken_at", taken_at, "extracted")?;
    }
    if let Some(width) = extracted.width {
        db::upsert_metadata(conn, entity_id, "width", &width.to_string(), "extracted")?;
    }
    if let Some(height) = extracted.height {
        db::upsert_metadata(conn, entity_id, "height", &height.to_string(), "extracted")?;
    }
    if let Some(make) = &extracted.camera_make {
        db::upsert_metadata(conn, entity_id, "camera_make", make, "extracted")?;
    }
    if let Some(model) = &extracted.camera_model {
        db::upsert_metadata(conn, entity_id, "camera_model", model, "extracted")?;
    }
    if let Some(lat) = &extracted.gps_latitude {
        db::upsert_metadata(conn, entity_id, "gps_latitude", lat, "extracted")?;
    }
    if let Some(lon) = &extracted.gps_longitude {
        db::upsert_metadata(conn, entity_id, "gps_longitude", lon, "extracted")?;
    }
    if let Some(modified_at) = fs_modified_at {
        db::upsert_metadata(conn, entity_id, "modified_at", modified_at, "extracted")?;
    }

    Ok(())
}

impl ExtractedImageMetadata {
    fn is_empty(&self) -> bool {
        self.width.is_none()
            && self.height.is_none()
            && self.taken_at.is_none()
            && self.camera_make.is_none()
            && self.camera_model.is_none()
            && self.gps_latitude.is_none()
            && self.gps_longitude.is_none()
    }
}

fn exif_ascii(exif: &exif::Exif, tag: Tag) -> Option<String> {
    exif.get_field(tag, In::PRIMARY).and_then(|field| match &field.value {
        Value::Ascii(values) => values
            .first()
            .map(|bytes| String::from_utf8_lossy(bytes).trim().to_string())
            .filter(|s| !s.is_empty()),
        _ => None,
    })
}

fn exif_datetime(exif: &exif::Exif, tag: Tag) -> Option<String> {
    let raw = exif_ascii(exif, tag)?;
    let naive = NaiveDateTime::parse_from_str(&raw, "%Y:%m:%d %H:%M:%S").ok()?;
    Some(Utc.from_utc_datetime(&naive).to_rfc3339())
}

fn exif_gps(exif: &exif::Exif) -> Option<(f64, f64)> {
    let lat = gps_coordinate(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef)?;
    let lon = gps_coordinate(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef)?;
    Some((lat, lon))
}

fn gps_coordinate(exif: &exif::Exif, value_tag: Tag, ref_tag: Tag) -> Option<f64> {
    let field = exif.get_field(value_tag, In::PRIMARY)?;
    let degrees = match &field.value {
        Value::Rational(values) if values.len() >= 3 => {
            values[0].to_f64() + values[1].to_f64() / 60.0 + values[2].to_f64() / 3600.0
        }
        _ => return None,
    };
    let negative = exif_ascii(exif, ref_tag)
        .map(|reference| matches!(reference.as_str(), "S" | "W"))
        .unwrap_or(false);
    Some(if negative { -degrees } else { degrees })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use tempfile::tempdir;

    #[test]
    fn extract_reads_png_dimensions() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("square.png");
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(640, 480, |_, _| Rgb([1, 2, 3]));
        img.save(&path).unwrap();

        let meta = extract(&path);
        assert_eq!(meta.width, Some(640));
        assert_eq!(meta.height, Some(480));
    }

    #[test]
    fn extract_ignores_non_image() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, b"hello").unwrap();
        assert!(extract(&path).is_empty());
    }

    #[test]
    fn persist_writes_width_and_height() {
        let dir = tempdir().unwrap();
        let vault = crate::Vault::init(dir.path().join("vault")).unwrap();
        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        db::insert_entity(
            vault.connection(),
            entity_id,
            None,
            Some("image/png"),
            0,
            "present",
            &now,
            None,
        )
        .unwrap();

        persist(
            vault.connection(),
            entity_id,
            &ExtractedImageMetadata {
                width: Some(1920),
                height: Some(1080),
                ..Default::default()
            },
            None,
        )
        .unwrap();

        let width: String = vault
            .connection()
            .query_row(
                "SELECT value FROM metadata WHERE entity_id = ?1 AND key = 'width'",
                [entity_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(width, "1920");
    }

    #[test]
    fn extract_reads_jpeg_dimensions() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(32, 32, |_, _| Rgb([4, 5, 6]));
        img.save(&path).unwrap();

        let meta = extract(&path);
        assert_eq!(meta.width, Some(32));
        assert_eq!(meta.height, Some(32));
    }
}
