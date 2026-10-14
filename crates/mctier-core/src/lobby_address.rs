use super::config_manager::EasyTierAdvancedConfig;
use sha2::{Digest, Sha256};

const SUBNET_PREFIX: &str = "10.126.126.";

fn configured_host(raw: &str) -> Result<u8, String> {
    let value = raw.trim();
    let (address, prefix) = value
        .split_once('/')
        .map_or((value, None), |(ip, prefix)| (ip, Some(prefix)));
    if prefix.is_some_and(|prefix| prefix != "24") {
        return Err(format!("虚拟 IP 必须使用固定网段 {SUBNET_PREFIX}0/24"));
    }
    let host = address
        .strip_prefix(SUBNET_PREFIX)
        .ok_or_else(|| format!("虚拟 IP 必须位于固定网段 {SUBNET_PREFIX}0/24"))?
        .parse::<u8>()
        .map_err(|_| "虚拟 IP 主机位必须是 1 到 254".to_string())?;
    if !(1..=254).contains(&host) {
        return Err("虚拟 IP 主机位必须是 1 到 254".into());
    }
    Ok(host)
}

/// Every candidate is visited once. Registration, not a hash, decides ownership.
pub fn candidate(lobby: &str, identity: &str, attempt: u16) -> Result<String, String> {
    if attempt >= 254 {
        return Err("大厅虚拟 IP 地址已耗尽".into());
    }
    let hash = Sha256::digest(format!("{identity}\n{lobby}").as_bytes());
    let seed = u32::from_be_bytes(hash[..4].try_into().unwrap());
    // Avoid the legacy creator's .1 on the first attempt, but include it in the scan.
    let first = 2 + seed % 253;
    Ok(format!(
        "10.126.126.{}",
        1 + (first - 1 + u32::from(attempt)) % 254
    ))
}

pub fn configuration(
    global: Option<&EasyTierAdvancedConfig>,
    lobby: Option<&EasyTierAdvancedConfig>,
    name: &str,
    identity: &str,
    attempt: u16,
) -> Result<(EasyTierAdvancedConfig, bool), String> {
    let mut config = lobby
        .filter(|c| !c.use_global_config)
        .or(global)
        .cloned()
        .unwrap_or_default();
    let configured = config.ipv4.as_deref().unwrap_or("").trim();
    let preferred_host = (!configured.is_empty())
        .then(|| configured_host(configured))
        .transpose()?;
    let automatic = configured.is_empty() || attempt != 0;
    if configured.is_empty() || attempt != 0 {
        config.ipv4 = Some(format!("{}/24", candidate(name, identity, attempt)?));
        // Static addresses also work when the first participant has no EasyTier peers.
        config.dhcp = false;
    } else {
        let host = preferred_host.expect("non-empty configuration has a validated host");
        config.ipv4 = Some(format!("{SUBNET_PREFIX}{host}/24"));
        config.dhcp = false;
    }
    config.use_global_config = false;
    Ok((config, automatic))
}


/// 校验大厅密码是否符合安全策略：
/// - 留空（或仅空白字符）表示无密码大厅，直接放行
/// - 非空时长度：8-32 个字符
/// - 非空时必须包含字母和数字
pub fn validate_password_policy(password: &str) -> Result<(), String> {
    let trimmed = password.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if trimmed.len() < 8 {
        return Err("密码至少需要 8 个字符".to_string());
    }
    if trimmed.len() > 32 {
        return Err("密码最多 32 个字符".to_string());
    }
    if !trimmed.chars().any(|c| c.is_alphabetic()) {
        return Err("密码必须包含至少一个字母".to_string());
    }
    if !trimmed.chars().any(|c| c.is_numeric()) {
        return Err("密码必须包含至少一个数字".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn address_candidates_cover_subnet_without_repetition() {
        // Shared vector with Android's LobbyAddressTest.
        assert_eq!(candidate("room", "alice", 0).unwrap(), "10.126.126.227");
        let ips: std::collections::HashSet<_> = (0..254)
            .map(|n| candidate("room", "alice", n).unwrap())
            .collect();
        assert_eq!(ips.len(), 254);
        assert!(ips.contains("10.126.126.1"));
        assert!(ips.contains("10.126.126.254"));
        assert!(!ips.contains("10.126.126.0"));
        assert!(candidate("room", "alice", 254).is_err());
        assert_ne!(candidate("room", "alice", 0).unwrap(), "10.126.126.1");
    }
    #[test]
    fn automatic_creation_and_join_work_without_dhcp_peers() {
        let (a, automatic) = configuration(None, None, "room", "alice", 0).unwrap();
        assert!(automatic);
        assert!(!a.dhcp);
        assert_eq!(
            a.ipv4,
            configuration(None, None, "room", "alice", 0)
                .unwrap()
                .0
                .ipv4
        );
        assert_ne!(
            a.ipv4,
            configuration(None, None, "room", "alice", 1)
                .unwrap()
                .0
                .ipv4
        );
    }
    #[test]
    fn manual_addresses_and_configuration_precedence_are_preserved() {
        let mut global = EasyTierAdvancedConfig::default();
        global.ipv4 = Some("10.126.126.20/24".into());
        let mut lobby = EasyTierAdvancedConfig::default();
        lobby.use_global_config = true;
        assert!(
            !configuration(Some(&global), Some(&lobby), "r", "a", 0)
                .unwrap()
                .1
        );
        let (fallback, automatic) =
            configuration(Some(&global), Some(&lobby), "r", "a", 1).unwrap();
        assert!(automatic);
        assert_ne!(fallback.ipv4, global.ipv4);
        lobby.use_global_config = false;
        assert!(
            configuration(Some(&global), Some(&lobby), "r", "a", 1)
                .unwrap()
                .1
        );
    }

    #[test]
    fn manual_address_is_limited_to_fixed_subnet_and_host_bits() {
        let mut global = EasyTierAdvancedConfig::default();
        global.ipv4 = Some("10.126.126.20/24".into());
        let (config, automatic) = configuration(Some(&global), None, "r", "a", 0).unwrap();
        assert!(!automatic);
        assert_eq!(config.ipv4.as_deref(), Some("10.126.126.20/24"));
        global.ipv4 = Some("10.144.144.20/24".into());
        assert!(configuration(Some(&global), None, "r", "a", 0).is_err());
    }
}
