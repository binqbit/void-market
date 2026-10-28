use super::{
    amount, inputs,
    model::{Actions, AuctionRow, AuctionView, Participant, Participation, SaleRequest},
};
use crate::{
    accounts::model::Account,
    http::error::{ApiError, Result},
    messages,
    state::AppState,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub async fn get(pool: &PgPool, id: Uuid) -> Result<AuctionRow> {
    sqlx::query_as("SELECT * FROM auctions WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn lock(connection: &mut PgConnection, id: Uuid) -> Result<AuctionRow> {
    sqlx::query_as("SELECT * FROM auctions WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(connection)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn participants(
    connection: &mut PgConnection,
    auction: Uuid,
) -> Result<Vec<Participant>> {
    Ok(sqlx::query_as(
        "SELECT * FROM auction_participants WHERE auction_id=$1 ORDER BY admission_seq",
    )
    .bind(auction)
    .fetch_all(connection)
    .await?)
}

pub async fn touch(connection: &mut PgConnection, auction: Uuid) -> Result<()> {
    sqlx::query("UPDATE auctions SET revision=nextval('message_revision') WHERE id=$1")
        .bind(auction)
        .execute(connection)
        .await?;
    Ok(())
}

async fn account(pool: &PgPool, account: Uuid) -> Result<Account> {
    Ok(
        sqlx::query_as("SELECT id AS address, name, username FROM accounts WHERE id=$1")
            .bind(account)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn view(state: &AppState, id: Uuid, viewer: Option<Uuid>) -> Result<AuctionView> {
    let mut connection = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *connection)
        .await?;
    let auction: AuctionRow = sqlx::query_as("SELECT * FROM auctions WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(ApiError::NotFound)?;
    let seller_view = viewer == Some(auction.seller_id);
    if matches!(
        auction.status.as_str(),
        "draft" | "cancelled" | "deposit_pending"
    ) && !seller_view
    {
        return Err(ApiError::NotFound);
    }
    let members = participants(&mut connection, id).await?;
    let own = members
        .iter()
        .find(|member| Some(member.account_id) == viewer);
    let buyers: Vec<&Participant> = members
        .iter()
        .filter(|member| member.account_id != auction.seller_id)
        .collect();
    let checks_completed: i64 = sqlx::query_scalar(
        "SELECT count(result) FROM auction_checks WHERE auction_id=$1 AND generation=$2",
    )
    .bind(id)
    .bind(auction.generation)
    .fetch_one(&mut *connection)
    .await?;
    let now = messages::now_ms()?;
    let healthy = super::clock::healthy(&mut connection, &auction, now).await?;
    let wallet_ready: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM custody_keys WHERE account_id=$1 AND he_status='ready')",
    )
    .bind(viewer)
    .fetch_one(&mut *connection)
    .await?;
    let settlement_started = if auction.execution == "localnet" {
        crate::settlement::auction::payment_started(&mut connection, id).await?
    } else {
        false
    };
    let payment = if auction.execution == "localnet"
        && (seller_view || viewer == auction.winner_id)
        && auction.winner_id.is_some()
    {
        let expected =
            crate::settlement::auction::amount_due(state, &mut connection, &auction).await?;
        let credited = crate::settlement::auction::paid(state, &mut connection, &auction).await?;
        Some(
            serde_json::json!({"expected":amount::format(expected),"paid":amount::format(credited),"remaining":amount::format(expected.saturating_sub(credited))}),
        )
    } else {
        None
    };
    connection.commit().await?;
    let active_count = buyers
        .iter()
        .filter(|member| member.status == "active")
        .count() as i64;
    let checks_total = active_count * (active_count + 1) / 2;

    let own_amount = own
        .map(|member| inputs::amount(state, id, member))
        .transpose()?;
    let agreed_price = if seller_view || (viewer.is_some() && viewer == auction.winner_id) {
        members
            .iter()
            .find(|member| Some(member.account_id) == auction.winner_id)
            .map(|member| inputs::amount(state, id, member).map(amount::format))
            .transpose()?
    } else {
        None
    };
    let own_active = own.is_some_and(|member| member.status == "active");
    let is_open = auction.status == "open" && auction.closes_at_ms > now;
    let editable = seller_view && auction.status == "draft";
    let actions = Actions {
        edit: editable,
        publish: editable,
        cancel: editable,
        bid: viewer.is_some()
            && !seller_view
            && own.is_none()
            && is_open
            && buyers.len() < auction.buyer_limit as usize,
        withdraw: !seller_view
            && own_active
            && !settlement_started
            && matches!(
                auction.status.as_str(),
                "open" | "matching" | "awaiting_completion"
            ),
        complete: seller_view
            && own_active
            && auction.status == "awaiting_completion"
            && (auction.execution == "internal"
                || (auction.payment_status == "confirmed" && auction.custody_status == "held")),
    };
    let participation = own.map(|member| {
        let running = member.chargeable && healthy && own_active && now >= auction.closes_at_ms;
        let elapsed = if running {
            now.saturating_sub(
                auction
                    .last_tick_ms
                    .unwrap_or(now)
                    .max(auction.closes_at_ms),
            )
            .clamp(0, 5000)
        } else {
            0
        };
        Participation {
            status: member.status.clone(),
            created_at_ms: member.created_at_ms,
            remaining_ms: member.remaining_ms.saturating_sub(elapsed).max(0),
            timer_running: running,
        }
    });
    let winner = if let Some(winner) = auction.winner_id {
        Some(account(&state.pool, winner).await?)
    } else {
        None
    };
    let seller = account(&state.pool, auction.seller_id).await?;
    Ok(AuctionView {
        id,
        asset: auction.asset_snapshot,
        seller,
        winner,
        status: auction.status,
        reason: auction.reason,
        currency: "ZEC",
        execution: if auction.execution == "localnet" {
            "localnet_custody"
        } else {
            "development_custody"
        },
        payment_status: auction.payment_status,
        custody_status: auction.custody_status,
        settlement_note: auction.settlement_note,
        payment,
        wallet_ready,
        closes_at_ms: auction.closes_at_ms,
        required_from_ms: auction.closes_at_ms,
        created_at_ms: auction.created_at_ms,
        published_at_ms: auction.published_at_ms,
        finished_at_ms: auction.finished_at_ms,
        revision: auction.revision,
        policy_hash: auction.policy_hash.map(|hash| URL_SAFE_NO_PAD.encode(hash)),
        buyer_limit: auction.buyer_limit as usize,
        admitted_buyers: buyers.len() as i64,
        active_buyers: active_count,
        generation: auction.generation,
        checks_completed,
        checks_total,
        reserve: if seller_view {
            own_amount.map(amount::format)
        } else {
            None
        },
        your_bid: if !seller_view {
            own_amount.map(amount::format)
        } else {
            None
        },
        agreed_price,
        participation,
        completion_request_id: if seller_view {
            auction.completion_request_id
        } else {
            None
        },
        actions,
    })
}

pub async fn list(
    state: &AppState,
    viewer: Option<Uuid>,
    mine: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuctionView>> {
    let ids: Vec<Uuid> = if mine {
        let account = viewer.ok_or(ApiError::Unauthorized)?;
        sqlx::query_scalar(
            "SELECT a.id FROM auctions a JOIN auction_participants p ON p.auction_id=a.id
             WHERE p.account_id=$1 ORDER BY a.created_at_ms DESC, a.id DESC LIMIT $2 OFFSET $3",
        )
        .bind(account)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT id FROM auctions WHERE status NOT IN ('draft','cancelled','deposit_pending')
             ORDER BY CASE WHEN status='open' THEN 0 ELSE 1 END, created_at_ms DESC, id DESC
             LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    };
    let mut views = Vec::with_capacity(ids.len());
    for id in ids {
        views.push(view(state, id, viewer).await?);
    }
    Ok(views)
}

pub async fn revision(pool: &PgPool, account: Uuid) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(max(a.revision),0) FROM auctions a JOIN auction_participants p
         ON p.auction_id=a.id WHERE p.account_id=$1",
    )
    .bind(account)
    .fetch_one(pool)
    .await?)
}

pub async fn pending_sales(state: &AppState, account: Uuid) -> Result<Vec<SaleRequest>> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM auctions WHERE seller_id=$1 AND status='awaiting_completion'
         ORDER BY created_at_ms LIMIT 100",
    )
    .bind(account)
    .fetch_all(&state.pool)
    .await?;
    let mut requests = Vec::new();
    for id in ids {
        let auction = view(state, id, Some(account)).await?;
        if !auction.actions.complete {
            continue;
        }
        let participant = auction.participation.ok_or(ApiError::Internal)?;
        requests.push(SaleRequest {
            revision: auction.revision,
            payment_status: auction.payment_status,
            auction_id: id,
            request_id: auction.completion_request_id.ok_or(ApiError::Internal)?,
            generation: auction.generation,
            title: auction.asset["name"]
                .as_str()
                .unwrap_or("Asset sale")
                .to_owned(),
            price: auction.agreed_price.ok_or(ApiError::Internal)?,
            buyer: serde_json::to_value(auction.winner).map_err(|_| ApiError::Internal)?,
            remaining_ms: participant.remaining_ms,
            timer_running: participant.timer_running,
        });
    }
    Ok(requests)
}
