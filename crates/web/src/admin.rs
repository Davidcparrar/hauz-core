//! The bill pages: `/admin`, `/admin/needs-review` and `/admin/bills/{id}`.

use hauz_core::bill::{Bill, BillId, Status};
use hauz_core::store::{BillStore, SqliteStore};
use topcoat::Result;
use topcoat::context::{Cx, app_context};
use topcoat::router::error::{RouterErrorExt, not_found};
use topcoat::router::{module_param, page, path_param};
use topcoat::view::{View, view};

use crate::format;

struct Row {
    href: String,
    prefix: String,
    vendor: String,
    amount: String,
    issued: String,
    due: String,
    status: &'static str,
}

impl Row {
    fn new(bill: &Bill) -> Self {
        Self {
            href: format!("/admin/bills/{}", bill.id().as_str()),
            prefix: format::id_prefix(bill),
            vendor: format::vendor(bill),
            amount: format::amount(bill),
            issued: format::date(bill.issued()),
            due: format::date(bill.due()),
            status: format::status(bill.status()),
        }
    }
}

/// Newest first, optionally only the bills needing review.
async fn rows(cx: &Cx, needs_review_only: bool) -> Result<Vec<Row>> {
    let bills = app_context::<SqliteStore>(cx).list().await?;
    Ok(bills
        .iter()
        .rev()
        .filter(|b| !needs_review_only || b.status() == Status::NeedsReview)
        .map(Row::new)
        .collect())
}

#[page]
async fn all(cx: &Cx) -> Result<impl View> {
    let rows = rows(cx, false).await?;
    Ok(view! {
        <h1>"Bills"</h1>
        if rows.is_empty() {
            <p class="empty">"no bills"</p>
        } else {
            <table>
                <tr>
                    <th>"Id"</th><th>"Vendor"</th><th>"Amount"</th>
                    <th>"Issued"</th><th>"Due"</th><th>"Status"</th>
                </tr>
                for r in rows {
                    <tr>
                        <td class="id"><a href=(r.href.as_str())>(r.prefix.as_str())</a></td>
                        <td>(r.vendor.as_str())</td>
                        <td>(r.amount.as_str())</td>
                        <td>(r.issued.as_str())</td>
                        <td>(r.due.as_str())</td>
                        <td>(r.status)</td>
                    </tr>
                }
            </table>
        }
    })
}

pub(crate) mod needs_review {
    use super::*;

    #[page]
    async fn only(cx: &Cx) -> Result<impl View> {
        let rows = rows(cx, true).await?;
        Ok(view! {
            <h1>"Bills needing review"</h1>
            if rows.is_empty() {
                <p class="empty">"no bills"</p>
            } else {
                <table>
                    <tr>
                        <th>"Id"</th><th>"Vendor"</th><th>"Amount"</th>
                        <th>"Issued"</th><th>"Due"</th><th>"Status"</th>
                    </tr>
                    for r in rows {
                        <tr>
                            <td class="id"><a href=(r.href.as_str())>(r.prefix.as_str())</a></td>
                            <td>(r.vendor.as_str())</td>
                            <td>(r.amount.as_str())</td>
                            <td>(r.issued.as_str())</td>
                            <td>(r.due.as_str())</td>
                            <td>(r.status)</td>
                        </tr>
                    }
                </table>
            }
        })
    }
}

pub(crate) mod bills {
    pub(crate) mod id {
        use super::super::*;

        module_param!(id);

        #[page]
        async fn detail(cx: &Cx) -> Result<impl View> {
            let id = BillId::new(path_param::<Id>(cx)).map_err(|_| not_found())?;
            let bill = app_context::<SqliteStore>(cx)
                .get(&id)
                .await?
                .ok_or_not_found()?;
            let fields = [
                ("Id", bill.id().as_str().to_owned()),
                ("Vendor", format::vendor(&bill)),
                ("Amount", format::amount(&bill)),
                ("Period", format::period(&bill)),
                ("Issued", format::date(bill.issued())),
                ("Due", format::date(bill.due())),
                ("Status", format::status(bill.status()).to_owned()),
            ];
            Ok(view! {
                <h1>"Bill"</h1>
                <dl>
                    for (name, value) in fields {
                        <dt>(name)</dt>
                        <dd>(value.as_str())</dd>
                    }
                </dl>
            })
        }
    }
}
