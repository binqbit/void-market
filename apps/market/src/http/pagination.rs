use serde::Deserialize;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl Page {
    pub fn bounds(&self) -> crate::http::error::Result<(i64, i64)> {
        let limit = self.limit.unwrap_or(20);
        let offset = self.offset.unwrap_or(0);
        if !(1..=100).contains(&limit) || !(0..=10_000).contains(&offset) {
            return Err(crate::http::error::ApiError::BadRequest(
                "Invalid page bounds",
            ));
        }
        Ok((limit, offset))
    }
}
