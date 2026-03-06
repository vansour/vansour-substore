/// 用户名最大长度限制
const MAX_USERNAME_LENGTH: usize = 64;

/// 用户名验证器
///
/// 验证用户名是否符合规范：
/// - 非空且不超过 64 个字符
/// - 只包含字母、数字、下划线和连字符
///
/// # 示例
///
/// ```
/// use vss_substore::utils::is_valid_username;
///
/// assert!(is_valid_username("user123"));
/// assert!(is_valid_username("user_name"));
/// assert!(is_valid_username("user-name"));
/// assert!(!is_valid_username("user@name"));
/// assert!(!is_valid_username(""));
/// assert!(!is_valid_username("a".repeat(65).as_str()));
/// ```
pub fn is_valid_username(username: &str) -> bool {
    if username.is_empty() || username.len() > MAX_USERNAME_LENGTH {
        return false;
    }
    username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}
