use crate::{
    errors::{
        Error,
        Result,
    },
    transport::{
        active_transactions::ActiveTransactions,
        messages::TransactionId,
    },
};
use futures::{
    TryFuture,
    TryFutureExt,
};
use krpc_encoding as proto;
use std::pin::Pin;
use tokio::prelude::{
    task::Context,
    *,
};

/// A future which resolves when the response for a transaction appears in a
/// peer's transaction map.
pub struct ResponseFuture {
    transaction_id: TransactionId,
    transactions: ActiveTransactions,
}

impl ResponseFuture {
    pub async fn wait_for_tx(
        transaction_id: TransactionId,
        transactions: ActiveTransactions,
    ) -> Result<proto::Envelope> {
        transactions.add_transaction(transaction_id)?;
        ResponseFuture::new(transaction_id, transactions)
            .into_future()
            .await
    }

    fn new(transaction_id: TransactionId, transactions: ActiveTransactions) -> ResponseFuture {
        ResponseFuture {
            transaction_id,
            transactions,
        }
    }
}

impl TryFuture for ResponseFuture {
    type Ok = proto::Envelope;
    type Error = Error;

    fn try_poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Self::Ok>> {
        self.transactions
            .poll_response(self.transaction_id, cx.waker())
    }
}

impl Drop for ResponseFuture {
    fn drop(&mut self) {
        self.transactions
            .drop_transaction(self.transaction_id)
            .unwrap();
    }
}
