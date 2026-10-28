use crate::{
    http::error::{ApiError, Result},
    state::AppState,
};
use actix_web::{HttpResponse, http::header, web};

pub async fn get(state: web::Data<AppState>, variant: web::Path<u8>) -> Result<HttpResponse> {
    if state.chains.is_none() || *variant > 5 {
        return Err(ApiError::NotFound);
    }
    let colors = [
        "#b5e885", "#78d6cb", "#a9a3e6", "#dfbd7c", "#8ebde8", "#cf95bb",
    ];
    let names = [
        "EVENT HORIZON",
        "QUIET SIGNAL",
        "NEON CIPHER",
        "DEEP ORBIT",
        "SILENT RELAY",
        "DARK MATTER",
    ];
    let index = usize::from(*variant);
    let color = colors[index];
    let angle = 17 + index * 31;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="640" height="640" viewBox="0 0 640 640">
<defs>
<radialGradient id="glow">
<stop stop-color="{color}" stop-opacity=".2"/>
<stop offset="1" stop-color="#0b0d11"/>
</radialGradient>
<linearGradient id="ring" x2="1" y2="1">
<stop stop-color="{color}"/>
<stop offset="1" stop-color="{color}" stop-opacity=".04"/>
</linearGradient>
</defs>
<rect width="640" height="640" fill="#0b0d11"/>
<circle cx="320" cy="292" r="285" fill="url(#glow)"/>
<g transform="rotate({angle} 320 292)" fill="none" stroke="url(#ring)">
<ellipse cx="320" cy="292" rx="223" ry="91" stroke-width="2"/>
<ellipse cx="320" cy="292" rx="91" ry="223" stroke-width="1"/>
<circle cx="320" cy="292" r="144" stroke-width="28"/>
<circle cx="320" cy="292" r="175" stroke-dasharray="2 12"/>
<path d="M48 292h544M320 28v528" stroke-opacity=".25"/>
</g>
<circle cx="320" cy="292" r="98" fill="#08090c"/>
<circle cx="320" cy="292" r="70" fill="none" stroke="{color}" stroke-opacity=".12"/>
<g fill="{color}">
<circle cx="134" cy="195" r="3"/>
<circle cx="510" cy="384" r="4"/>
<circle cx="401" cy="96" r="2"/>
</g>
<text x="42" y="52" fill="#b1b6bd" font-family="monospace" font-size="11" letter-spacing="4">VOID / LOCAL COLLECTION</text>
<text x="42" y="573" fill="{color}" font-family="monospace" font-size="20" letter-spacing="4">{name}</text>
<text x="42" y="601" fill="#6f7884" font-family="monospace" font-size="10" letter-spacing="3">ERC-721 · DEMO ARTIFACT 0{number}</text>
</svg>"##,
        name = names[index],
        number = index + 1
    );
    Ok(HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "image/svg+xml"))
        .insert_header((header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .insert_header((header::CACHE_CONTROL, "public, max-age=86400"))
        .insert_header((
            "Content-Security-Policy",
            "default-src 'none'; style-src 'unsafe-inline'",
        ))
        .body(svg))
}
