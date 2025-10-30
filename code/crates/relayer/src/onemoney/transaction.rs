use onemoney_protocol::client::http::Client;
use onemoney_protocol::responses::CheckpointTransactions;
use onemoney_protocol::Transaction;

use crate::onemoney::error::Error;

pub async fn get_transactions_from_checkpoint<FilterFn>(
    url: String,
    checkpoint_number: u64,
    filter: FilterFn,
) -> Result<Vec<Transaction>, Error>
where
    FilterFn: Fn(&Transaction) -> bool,
{
    let client = Client::custom(url)?;
    let checkpoint = client
        .get_checkpoint_by_number(checkpoint_number, true)
        .await?;

    match checkpoint
        .transactions {
            CheckpointTransactions::Full(transactions) => Ok(transactions
                .into_iter()
                .filter(filter)
                .collect()),
            CheckpointTransactions::Hashes(_) => Err(Error::Generic(format!("Checkpoint {checkpoint_number} contains hashed transactions instead of full transactions"))),
        }
}
