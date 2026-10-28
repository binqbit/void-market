use crate::{
    auth::sessions::Principal,
    http::error::{ApiError, Result},
    state::AppState,
};
use actix_multipart::Multipart;
use actix_web::{HttpResponse, http::header, web};
use futures_util::TryStreamExt;
use image::ImageFormat;
use serde::Serialize;
use std::io::Cursor;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const MAX_UPLOAD: usize = 10 * 1024 * 1024;

#[derive(Serialize)]
pub struct UploadedImage {
    pub image: String,
    pub url: String,
}

pub async fn upload(
    state: web::Data<AppState>,
    actor: Principal,
    mut multipart: Multipart,
) -> Result<HttpResponse> {
    let mut field = multipart
        .try_next()
        .await
        .map_err(|_| ApiError::BadRequest("Invalid multipart upload"))?
        .ok_or(ApiError::BadRequest("One image file is required"))?;
    if field.name() != Some("file") {
        return Err(ApiError::BadRequest("Expected a file field"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = field
        .try_next()
        .await
        .map_err(|_| ApiError::BadRequest("Invalid image upload"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_UPLOAD {
            return Err(ApiError::PayloadTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    drop(field);
    if multipart
        .try_next()
        .await
        .map_err(|_| ApiError::BadRequest("Invalid upload"))?
        .is_some()
    {
        return Err(ApiError::BadRequest("Upload exactly one image"));
    }

    let format =
        image::guess_format(&bytes).map_err(|_| ApiError::BadRequest("Unsupported image"))?;
    let extension = match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::WebP => "webp",
        _ => return Err(ApiError::BadRequest("Use PNG, JPEG or WebP")),
    };
    let (width, height) = image::ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| ApiError::BadRequest("Invalid image"))?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 20_000_000 {
        return Err(ApiError::BadRequest("Image dimensions exceed the limit"));
    }

    let name = format!("{}_{}.{}", actor.0, Uuid::new_v4(), extension);
    let temporary = state.config.media_root.join(format!("{name}.part"));
    let destination = state.config.media_root.join(&name);
    let mut file = tokio::fs::File::create(&temporary).await?;
    file.write_all(&bytes).await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(&temporary, &destination).await?;
    tokio::fs::File::open(&state.config.media_root)
        .await?
        .sync_all()
        .await?;
    Ok(HttpResponse::Ok().json(UploadedImage {
        url: format!("/v1/media/{name}"),
        image: name,
    }))
}

pub async fn validate_owned_image(state: &AppState, actor: Uuid, name: &str) -> Result<()> {
    let (owner, _) = parse_name(name)?;
    if owner != actor {
        return Err(ApiError::Forbidden);
    }
    if !tokio::fs::try_exists(state.config.media_root.join(name)).await? {
        return Err(ApiError::BadRequest("Upload the image first"));
    }
    Ok(())
}

pub async fn get(state: web::Data<AppState>, name: web::Path<String>) -> Result<HttpResponse> {
    let name = name.into_inner();
    let (_, mime) = parse_name(&name)?;
    let bytes = tokio::fs::read(state.config.media_root.join(name))
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ApiError::NotFound
            } else {
                error.into()
            }
        })?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, mime))
        .insert_header((header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .insert_header((header::CACHE_CONTROL, "public, max-age=31536000, immutable"))
        .body(bytes))
}

fn parse_name(name: &str) -> Result<(Uuid, &'static str)> {
    let (stem, extension) = name.rsplit_once('.').ok_or(ApiError::NotFound)?;
    let (owner, id) = stem.split_once('_').ok_or(ApiError::NotFound)?;
    let owner = Uuid::parse_str(owner).map_err(|_| ApiError::NotFound)?;
    let id = Uuid::parse_str(id).map_err(|_| ApiError::NotFound)?;
    let mime = match extension {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return Err(ApiError::NotFound),
    };
    if name != format!("{owner}_{id}.{extension}") {
        return Err(ApiError::NotFound);
    }
    Ok((owner, mime))
}
