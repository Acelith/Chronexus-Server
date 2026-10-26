use crate::{
    auth::now,
    campaigns::{identifier, member, missing, timestamp},
    content::writable,
    db::Database,
    error::{ApiError, Result},
};
use axum::http::StatusCode;
use image::{ImageDecoder, ImageFormat, ImageReader};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use utoipa::ToSchema;
use uuid::Uuid;

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub author_id: String,
    pub sha256: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub byte_length: u64,
    pub created_at: String,
}
pub struct Download {
    pub metadata: Attachment,
    pub bytes: Vec<u8>,
}
fn decode(bytes: &[u8], mime: &str, checksum: &str) -> Result<(u32, u32)> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Image must contain 1 byte to 8 MiB.",
        ));
    }
    if checksum.len() != 64
        || !checksum
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || format!("{:x}", Sha256::digest(bytes)) != checksum
    {
        return Err(ApiError::invalid());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ApiError::invalid())?;
    let expected = match reader.format() {
        Some(ImageFormat::Png) => "image/png",
        Some(ImageFormat::Jpeg) => "image/jpeg",
        Some(ImageFormat::WebP) => "image/webp",
        _ => return Err(ApiError::invalid()),
    };
    if expected != mime {
        return Err(ApiError::invalid());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|_| ApiError::invalid())?;
    let (w, h) = decoder.dimensions();
    if w == 0
        || h == 0
        || u64::from(w) * u64::from(h) > 16_000_000
        || decoder.total_bytes() > 64 * 1024 * 1024
    {
        return Err(ApiError::invalid());
    }
    // Validate a complete decodable image; persistence keeps the submitted original bytes.
    image::DynamicImage::from_decoder(decoder).map_err(|_| ApiError::invalid())?;
    Ok((w, h))
}
impl Database {
    pub fn upload_attachment(
        &self,
        token: &str,
        campaign: &str,
        bytes: &[u8],
        mime: &str,
        checksum: &str,
    ) -> Result<Attachment> {
        let identity = self.identity(token)?;
        self.with(|conn| writable(conn, campaign, &identity.account_id))?;
        let (width, height) = decode(bytes, mime, checksum)?;
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;writable(&tx,campaign,&identity.account_id)?;
            let (count,campaign_bytes):(i64,i64)=tx.query_row("SELECT count(*),coalesce(sum(byte_length),0) FROM attachments WHERE campaign_id=?1",[campaign],|r|Ok((r.get(0)?,r.get(1)?)))?;
            let account_bytes:i64=tx.query_row("SELECT coalesce(sum(byte_length),0) FROM attachments WHERE author_id=?1",[&identity.account_id],|r|r.get(0))?;
            if count>=512||campaign_bytes+bytes.len() as i64>128*1024*1024||account_bytes+bytes.len() as i64>256*1024*1024{return Err(ApiError(StatusCode::TOO_MANY_REQUESTS,"Attachment quota reached."));}
            let id=Uuid::new_v4().to_string();let time=now();
            tx.execute("INSERT INTO attachments(id,campaign_id,author_id,sha256,mime,width,height,byte_length,original,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![id,campaign,identity.account_id,checksum,mime,width,height,bytes.len() as i64,bytes,time])?;
            tx.commit()?;Ok(Attachment{id,author_id:identity.account_id.clone(),sha256:checksum.into(),mime:mime.into(),width,height,byte_length:bytes.len() as u64,created_at:timestamp(time)})
        })
    }
    pub fn download_attachment(&self, token: &str, campaign: &str, id: &str) -> Result<Download> {
        identifier(id)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            member(conn,campaign,&identity.account_id)?;
            let accessible:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM attachments a WHERE a.id=?1 AND a.campaign_id=?2 AND (a.author_id=?3 OR EXISTS(SELECT 1 FROM content_attachments r JOIN content c ON c.campaign_id=r.campaign_id AND c.kind=r.kind AND c.id=r.item_id WHERE r.attachment_id=a.id AND c.campaign_id=?2 AND c.visibility='members' AND c.deleted=0)))",params![id,campaign,identity.account_id],|r|r.get(0))?;
            if !accessible{return Err(missing());}
            let row=conn.query_row("SELECT id,author_id,sha256,mime,width,height,byte_length,created_at,original FROM attachments WHERE id=?1 AND campaign_id=?2",params![id,campaign],|r|Ok((Attachment{id:r.get(0)?,author_id:r.get(1)?,sha256:r.get(2)?,mime:r.get(3)?,width:r.get(4)?,height:r.get(5)?,byte_length:r.get::<_,i64>(6)? as u64,created_at:timestamp(r.get(7)?)},r.get::<_,Vec<u8>>(8)?)))?;
            if format!("{:x}",Sha256::digest(&row.1))!=row.0.sha256{return Err(ApiError::internal());}
            Ok(Download{metadata:row.0,bytes:row.1})
        })
    }
    pub fn delete_attachment(&self, token: &str, campaign: &str, id: &str) -> Result<()> {
        identifier(id)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            writable(&tx, campaign, &identity.account_id)?;
            let author: Option<String> = tx
                .query_row(
                    "SELECT author_id FROM attachments WHERE id=?1 AND campaign_id=?2",
                    params![id, campaign],
                    |r| r.get(0),
                )
                .optional()?;
            if author.as_deref() != Some(&identity.account_id) {
                return Err(missing());
            }
            let refs: i64 = tx.query_row(
                "SELECT count(*) FROM content_attachments WHERE attachment_id=?1",
                [id],
                |r| r.get(0),
            )?;
            if refs != 0 {
                return Err(ApiError(
                    StatusCode::CONFLICT,
                    "Remove image references before deleting the original.",
                ));
            }
            tx.execute(
                "DELETE FROM attachments WHERE id=?1 AND campaign_id=?2",
                params![id, campaign],
            )?;
            tx.commit()?;
            Ok(())
        })
    }
}
