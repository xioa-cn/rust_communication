pub(super) const MAX_ITEMS: usize = 1024;

pub(super) fn validate_address(address: &str) -> Result<(), String> {
    if address.trim().is_empty() || address.len() > 128 {
        return Err("地址不能为空，且不得超过 128 字节".into());
    }
    Ok(())
}
pub(super) fn validate_count(count: usize) -> Result<(), String> {
    if !(1..=MAX_ITEMS).contains(&count) {
        return Err(format!("本示例单次操作数量必须在 1..{MAX_ITEMS} 之间"));
    }
    Ok(())
}
