use super::{
    ceremonies::Ceremony,
    model::{LoginFinish, LoginOptions, Options, RegisterFinish, RegisterOptions},
    sessions,
};
use crate::{
    accounts::{model::Account, repository},
    http::error::{ApiError, Result},
    state::AppState,
};
use actix_web::http::header::HeaderValue;
use serde_json::Value;
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

pub async fn register_options(state: &AppState, input: RegisterOptions) -> Result<Options> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(ApiError::BadRequest("Name must contain 1 to 64 characters"));
    }
    let account_id = Uuid::new_v4();
    let (challenge, registration) = state
        .webauthn
        .start_passkey_registration(account_id, &account_id.to_string(), name, None)
        .map_err(|_| ApiError::Internal)?;
    let ceremony_id = state.ceremonies.insert(Ceremony::Registration {
        account_id,
        name: name.to_owned(),
        state: registration,
    })?;
    Ok(Options {
        ceremony_id,
        address: account_id,
        public_key: serde_json::to_value(challenge).map_err(|_| ApiError::Internal)?,
    })
}

pub async fn register_finish(
    state: &AppState,
    input: RegisterFinish,
) -> Result<(HeaderValue, Account)> {
    let Ceremony::Registration {
        account_id,
        name,
        state: registration,
    } = state.ceremonies.take(input.ceremony_id)?
    else {
        return Err(ApiError::BadRequest("Wrong ceremony type"));
    };
    let passkey = state
        .webauthn
        .finish_passkey_registration(&input.credential, &registration)
        .map_err(|_| ApiError::BadRequest("Passkey registration could not be verified"))?;
    let credential_id: &[u8] = passkey.cred_id().as_ref();
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO accounts(id, name, credential_id, passkey)
         VALUES ($1,$2,$3,$4)",
    )
    .bind(account_id)
    .bind(&name)
    .bind(credential_id)
    .bind(serde_json::to_value(&passkey).map_err(|_| ApiError::Internal)?)
    .execute(&mut *tx)
    .await?;
    let cookie = sessions::issue(
        &mut tx,
        account_id,
        state.config.secure_cookie,
        &state.config.jwt_secret,
    )
    .await?;
    tx.commit().await?;
    Ok((
        cookie,
        Account {
            address: account_id,
            name,
            username: None,
        },
    ))
}

pub async fn login_options(state: &AppState, input: LoginOptions) -> Result<Options> {
    let stored: Value = sqlx::query_scalar(
        "SELECT passkey
         FROM accounts
         WHERE id = $1",
    )
    .bind(input.address)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let passkey: Passkey =
        serde_json::from_value(stored.clone()).map_err(|_| ApiError::Internal)?;
    let (challenge, authentication) = state
        .webauthn
        .start_passkey_authentication(&[passkey])
        .map_err(|_| ApiError::Internal)?;
    let ceremony_id = state.ceremonies.insert(Ceremony::Authentication {
        account_id: input.address,
        state: authentication,
        credential_snapshot: stored,
    })?;
    Ok(Options {
        ceremony_id,
        address: input.address,
        public_key: serde_json::to_value(challenge).map_err(|_| ApiError::Internal)?,
    })
}

pub async fn login_finish(state: &AppState, input: LoginFinish) -> Result<(HeaderValue, Account)> {
    let Ceremony::Authentication {
        account_id,
        state: authentication,
        credential_snapshot,
    } = state.ceremonies.take(input.ceremony_id)?
    else {
        return Err(ApiError::BadRequest("Wrong ceremony type"));
    };
    let verified = state
        .webauthn
        .finish_passkey_authentication(&input.credential, &authentication)
        .map_err(|_| ApiError::Unauthorized)?;

    let mut tx = state.pool.begin().await?;
    let stored: Value = sqlx::query_scalar(
        "SELECT passkey
         FROM accounts
         WHERE id = $1 FOR UPDATE",
    )
    .bind(account_id)
    .fetch_one(&mut *tx)
    .await?;
    if stored != credential_snapshot {
        return Err(ApiError::Conflict(
            "Credential changed; start a new login ceremony",
        ));
    }
    let mut passkey: Passkey = serde_json::from_value(stored).map_err(|_| ApiError::Internal)?;
    passkey
        .update_credential(&verified)
        .ok_or(ApiError::Unauthorized)?;
    sqlx::query(
        "UPDATE accounts SET passkey = $1
         WHERE id = $2",
    )
    .bind(serde_json::to_value(passkey).map_err(|_| ApiError::Internal)?)
    .bind(account_id)
    .execute(&mut *tx)
    .await?;
    let cookie = sessions::issue(
        &mut tx,
        account_id,
        state.config.secure_cookie,
        &state.config.jwt_secret,
    )
    .await?;
    tx.commit().await?;

    let account = repository::get(&state.pool, account_id).await?;
    Ok((cookie, account))
}
