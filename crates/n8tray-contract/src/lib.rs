//! The N8 Tray contract: the manifest an app installs, the status it reports
//! and the lease the host writes.

/// The contract major version this crate implements.
pub const CONTRACT: u32 = 2;

#[cfg(test)]
mod tests {
    #[test]
    fn implements_contract_2() {
        assert_eq!(super::CONTRACT, 2);
    }
}
