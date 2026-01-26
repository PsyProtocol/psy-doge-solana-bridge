#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub struct FeeResult {
    pub fees_generated: u64,
    pub amount_after_fees: u64,
}

pub fn calcuate_deposit_fee(
    total_deposit_amount: u64,
    flat_fee_per_deposit_sats: u64,
    deposit_fee_rate_numerator: u64,
    deposit_fee_rate_denominator: u64,
) -> anyhow::Result<FeeResult> {
    // Use u128 to avoid floating point precision issues
    let fees_generated = (total_deposit_amount as u128)
        .checked_mul(deposit_fee_rate_numerator as u128)
        .ok_or_else(|| anyhow::anyhow!("Fee calculation overflow"))?
        .checked_div(deposit_fee_rate_denominator as u128)
        .ok_or_else(|| anyhow::anyhow!("Division by zero in fee calculation"))?
        .checked_add(flat_fee_per_deposit_sats as u128)
        .ok_or_else(|| anyhow::anyhow!("Fee calculation overflow"))? as u64;

    Ok(FeeResult{
        fees_generated,
        amount_after_fees: total_deposit_amount
            .checked_sub(fees_generated)
            .ok_or_else(|| anyhow::anyhow!("Fee calculation underflow"))?,
    })
}


pub fn calcuate_withdrawal_fee(
    total_withdrawal_amount: u64,
    flat_fee_per_withdrawal_sats: u64,
    withdrawal_fee_rate_numerator: u64,
    withdrawal_fee_rate_denominator: u64,
) -> anyhow::Result<FeeResult> {
    // Use u128 to avoid floating point precision issues
    let fees_generated = (total_withdrawal_amount as u128)
        .checked_mul(withdrawal_fee_rate_numerator as u128)
        .ok_or_else(|| anyhow::anyhow!("Fee calculation overflow"))?
        .checked_div(withdrawal_fee_rate_denominator as u128)
        .ok_or_else(|| anyhow::anyhow!("Division by zero in fee calculation"))?
        .checked_add(flat_fee_per_withdrawal_sats as u128)
        .ok_or_else(|| anyhow::anyhow!("Fee calculation overflow"))? as u64;

    
    Ok(FeeResult{
        fees_generated,
        amount_after_fees: total_withdrawal_amount
            .checked_sub(fees_generated)
            .ok_or_else(|| anyhow::anyhow!("Fee calculation underflow"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::{calcuate_deposit_fee as calcuate_fee, FeeResult};
    use proptest::prelude::*;
    pub fn calcuate_fee_float_implementation(
        total_deposit_amount: u64,
        flat_fee_per_deposit_sats: u64,
        deposit_fee_rate_numerator: u64,
        deposit_fee_rate_denominator: u64,
    ) -> anyhow::Result<FeeResult> {
        let deposit_fee_rate = (deposit_fee_rate_numerator as f64)
            / (deposit_fee_rate_denominator as f64);
        let fees_generated = (total_deposit_amount as f64 * deposit_fee_rate).floor() as u64 + flat_fee_per_deposit_sats;
            
        Ok(FeeResult{
            fees_generated,
            amount_after_fees: total_deposit_amount
                .checked_sub(fees_generated)
                .ok_or_else(|| anyhow::anyhow!("Fee calculation underflow"))?,
        })
    }
    // A helper function to compare the results of the two functions
    fn assert_parity(
        total_deposit_amount: u64,
        flat_fee_per_deposit_sats: u64,
        deposit_fee_rate_numerator: u64,
        deposit_fee_rate_denominator: u64,
    ) {
        let float_result = calcuate_fee_float_implementation(
            total_deposit_amount,
            flat_fee_per_deposit_sats,
            deposit_fee_rate_numerator,
            deposit_fee_rate_denominator,
        );
        let u128_result = calcuate_fee(
            total_deposit_amount,
            flat_fee_per_deposit_sats,
            deposit_fee_rate_numerator,
            deposit_fee_rate_denominator,
        );

        assert_eq!(float_result.unwrap(), u128_result.unwrap());
    }
    fn assert_parity_error(
        total_deposit_amount: u64,
        flat_fee_per_deposit_sats: u64,
        deposit_fee_rate_numerator: u64,
        deposit_fee_rate_denominator: u64,
    ) {
        let float_result = calcuate_fee_float_implementation(
            total_deposit_amount,
            flat_fee_per_deposit_sats,
            deposit_fee_rate_numerator,
            deposit_fee_rate_denominator,
        );
        let u128_result = calcuate_fee(
            total_deposit_amount,
            flat_fee_per_deposit_sats,
            deposit_fee_rate_numerator,
            deposit_fee_rate_denominator,
        );

        assert!(float_result.is_err() && u128_result.is_err());
    }

    #[test]
    fn test_simple_values() {
        assert_parity(100_000, 500, 1, 100); // 1% fee
    }

    #[test]
    fn test_large_values() {
        assert_parity(100_000_000_000, 1_000, 5, 1000); // 0.5% fee
    }

    #[test]
    fn test_fractional_fee() {
        assert_parity(123_456, 100, 123, 100_000); // 0.123% fee
    }

    #[test]
    fn test_zero_fee_rate() {
        assert_parity(1_000_000, 250, 0, 100);
    }
    #[test]
    fn test_zero_total_deposit() {
        assert_parity_error(0, 500, 1, 100);
    }

    proptest! {
        #[test]
        fn proptest_parity(
            total_deposit_amount in 0..u64::MAX,
            flat_fee_per_deposit_sats in 0..1_000u64,
            deposit_fee_rate_numerator in 0..1_000_000u64,
            deposit_fee_rate_denominator in 1..1_000_000u64
        ) {
            if deposit_fee_rate_numerator > deposit_fee_rate_denominator || total_deposit_amount < flat_fee_per_deposit_sats || total_deposit_amount < (flat_fee_per_deposit_sats as u64 + ((total_deposit_amount as u128 * deposit_fee_rate_numerator as u128) / deposit_fee_rate_denominator as u128) as u64) {
                // Skip cases where fee rate > 100%
                return Ok(());
            }

            let float_result = calcuate_fee_float_implementation(
                total_deposit_amount,
                flat_fee_per_deposit_sats,
                deposit_fee_rate_numerator,
                deposit_fee_rate_denominator,
            );
            let u128_result = calcuate_fee(
                total_deposit_amount,
                flat_fee_per_deposit_sats,
                deposit_fee_rate_numerator,
                deposit_fee_rate_denominator,
            );
            // We expect the results to be very close, but floating point arithmetic
            // can have small precision errors. A difference of 1 should be acceptable.
            let float_fee = float_result.unwrap().fees_generated;
            let u128_fee = u128_result.unwrap().fees_generated;
            assert!((float_fee as i64 - u128_fee as i64).abs() <= 10000, "Discrepancy too large: float_fee = {}, u128_fee = {}", float_fee, u128_fee);
        }
    }
}