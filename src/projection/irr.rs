//! Internal Rate of Return (IRR) calculation
//!
//! Used to calculate Cost of Funds from projection cashflows

/// Calculate the Internal Rate of Return (IRR) for a series of cash flows
/// using the Newton-Raphson method.
///
/// # Arguments
/// * `cashflows` - Vector of cash flows (positive = inflow, negative = outflow)
/// * `periods_per_year` - Number of periods per year (12 for monthly)
///
/// # Returns
/// * `Option<f64>` - Annual IRR as a decimal (e.g., 0.05 for 5%), or None if no solution found
pub fn calculate_irr(cashflows: &[f64], periods_per_year: u32) -> Option<f64> {
    // Handle edge cases
    if cashflows.is_empty() {
        return None;
    }

    // Check if all cashflows are zero
    if cashflows.iter().all(|&cf| cf.abs() < 1e-10) {
        return Some(0.0);
    }

    // Check if there's at least one sign change (required for IRR to exist)
    let has_positive = cashflows.iter().any(|&cf| cf > 1e-10);
    let has_negative = cashflows.iter().any(|&cf| cf < -1e-10);
    if !has_positive || !has_negative {
        return None; // No sign change means no IRR
    }

    // Newton-Raphson iteration for periodic (monthly) rate
    let mut rate = 0.05 / periods_per_year as f64; // Initial guess: 5% annual / periods
    let tolerance = 1e-10;
    let max_iterations = 1000;

    for _ in 0..max_iterations {
        let (npv, dnpv) = npv_and_derivative(cashflows, rate);

        if dnpv.abs() < 1e-20 {
            // Derivative too small, try bisection instead
            return calculate_irr_bisection(cashflows, periods_per_year);
        }

        let new_rate = rate - npv / dnpv;

        // Bound the rate to reasonable values
        let new_rate = new_rate.max(-0.99).min(10.0);

        if (new_rate - rate).abs() < tolerance {
            // Convert periodic rate to annual rate
            let annual_rate = (1.0 + new_rate).powi(periods_per_year as i32) - 1.0;
            return Some(annual_rate);
        }

        rate = new_rate;
    }

    // Newton-Raphson didn't converge, try bisection
    calculate_irr_bisection(cashflows, periods_per_year)
}

/// Calculate NPV and its derivative with respect to rate
fn npv_and_derivative(cashflows: &[f64], rate: f64) -> (f64, f64) {
    let mut npv = 0.0;
    let mut dnpv = 0.0;

    for (t, &cf) in cashflows.iter().enumerate() {
        let discount = (1.0 + rate).powi(t as i32);
        npv += cf / discount;
        if t > 0 {
            dnpv -= (t as f64) * cf / ((1.0 + rate).powi(t as i32 + 1));
        }
    }

    (npv, dnpv)
}

/// Fallback IRR calculation using bisection method
fn calculate_irr_bisection(cashflows: &[f64], periods_per_year: u32) -> Option<f64> {
    let mut low = -0.99_f64;  // -99% periodic rate
    let mut high = 10.0_f64;  // 1000% periodic rate
    let tolerance = 1e-10;
    let max_iterations = 1000;

    let npv_low = npv_at_rate(cashflows, low);
    let npv_high = npv_at_rate(cashflows, high);

    // Check that we have a root in this interval
    if npv_low * npv_high > 0.0 {
        return None;
    }

    for _ in 0..max_iterations {
        let mid = (low + high) / 2.0;
        let npv_mid = npv_at_rate(cashflows, mid);

        if npv_mid.abs() < tolerance || (high - low) / 2.0 < tolerance {
            // Convert periodic rate to annual rate
            let annual_rate = (1.0 + mid).powi(periods_per_year as i32) - 1.0;
            return Some(annual_rate);
        }

        if npv_mid * npv_at_rate(cashflows, low) < 0.0 {
            high = mid;
        } else {
            low = mid;
        }
    }

    None
}

/// Calculate NPV at a given periodic rate
fn npv_at_rate(cashflows: &[f64], rate: f64) -> f64 {
    cashflows
        .iter()
        .enumerate()
        .map(|(t, &cf)| cf / (1.0 + rate).powi(t as i32))
        .sum()
}

/// Calculate Cost of Funds from projection net cashflows
///
/// Cost of Funds is defined as the IRR of the net cashflows, expressed as an annual rate.
/// This represents the effective cost of the liabilities to the insurance company.
pub fn calculate_cost_of_funds(net_cashflows: &[f64]) -> Option<f64> {
    calculate_irr(net_cashflows, 12) // Monthly cashflows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_irr() {
        // Investment of $1000, returns $1100 after 1 year (monthly)
        let mut cashflows = vec![-1000.0];
        cashflows.extend(vec![0.0; 11]);
        cashflows.push(1100.0);

        let irr = calculate_irr(&cashflows, 12).unwrap();
        assert!((irr - 0.10).abs() < 0.001, "Expected ~10% IRR, got {}", irr);
    }

    #[test]
    fn test_level_cashflows() {
        // Loan of $10000, 12 monthly payments of $900
        let mut cashflows = vec![10000.0];
        cashflows.extend(vec![-900.0; 12]);

        let irr = calculate_irr(&cashflows, 12);
        assert!(irr.is_some());
    }
}
