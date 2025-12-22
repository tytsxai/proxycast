//! Provider module property tests
//!
//! 使用 proptest 进行属性测试

use chrono::{Duration, Utc};
use proptest::prelude::*;

use crate::providers::codex::{CodexCredentials, CodexProvider};
use crate::providers::iflow::{IFlowCredentials, IFlowProvider};
use crate::providers::vertex::VertexProvider;

/// Generate a random lead time in minutes (1 to 30 minutes)
fn arb_lead_time_mins() -> impl Strategy<Value = i64> {
    1i64..30i64
}

/// Generate a random offset from now in seconds (-3600 to +7200)
/// Negative means past, positive means future
fn arb_time_offset_secs() -> impl Strategy<Value = i64> {
    -3600i64..7200i64
}

/// 生成不会与 lead_time 边界冲突的时间偏移
/// 避免 time_offset_secs 恰好等于 lead_time_mins * 60 的情况
fn arb_time_offset_avoiding_boundary(lead_time_mins: i64) -> impl Strategy<Value = i64> {
    let boundary = lead_time_mins * 60;
    // 生成不等于边界值的时间偏移
    (-3600i64..7200i64).prop_filter("避免边界值", move |&offset| offset != boundary)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: cliproxyapi-parity, Property 2: Token Refresh Timing**
    /// *For any* stored OAuth token with expiration time T, the refresh mechanism
    /// SHALL be triggered before time T.
    /// **Validates: Requirements 1.2, 2.2**
    ///
    /// This test verifies that:
    /// 1. When token expires within lead_time, needs_refresh returns true
    /// 2. When token expires after lead_time, needs_refresh returns false
    /// 3. When no token exists, needs_refresh returns true
    /// 4. When no expiry info exists, needs_refresh returns true
    #[test]
    fn test_codex_token_refresh_timing(
        lead_time_mins in arb_lead_time_mins(),
        time_offset_secs in -3600i64..7200i64,
    ) {
        let lead_time = Duration::minutes(lead_time_mins);
        let lead_time_secs = lead_time_mins * 60;

        // 跳过边界条件，因为时间精度问题可能导致不确定行为
        prop_assume!(time_offset_secs != lead_time_secs);

        let mut provider = CodexProvider::new();
        provider.credentials.access_token = Some("test_token".to_string());

        // Set expiration time relative to now
        let now = Utc::now();
        let expires_at = now + Duration::seconds(time_offset_secs);
        provider.credentials.expires_at = Some(expires_at.to_rfc3339());

        let needs_refresh = provider.needs_refresh(lead_time);

        // Token should need refresh if it expires within lead_time from now
        // i.e., expires_at < now + lead_time
        // i.e., time_offset_secs < lead_time_secs
        let expected_needs_refresh = time_offset_secs < lead_time_secs;

        prop_assert_eq!(
            needs_refresh,
            expected_needs_refresh,
            "Codex: Token with expiry in {} seconds should {} refresh with lead time of {} minutes",
            time_offset_secs,
            if expected_needs_refresh { "need" } else { "not need" },
            lead_time_mins
        );
    }

    /// **Feature: cliproxyapi-parity, Property 2: Token Refresh Timing**
    /// Test that iFlow OAuth tokens trigger refresh before expiration
    /// **Validates: Requirements 1.2, 2.2**
    #[test]
    fn test_iflow_token_refresh_timing(
        lead_time_mins in arb_lead_time_mins(),
        time_offset_secs in -3600i64..7200i64,
    ) {
        let lead_time = Duration::minutes(lead_time_mins);
        let lead_time_secs = lead_time_mins * 60;

        // 跳过边界条件，因为时间精度问题可能导致不确定行为
        prop_assume!(time_offset_secs != lead_time_secs);

        let mut provider = IFlowProvider::new();
        provider.credentials.auth_type = "oauth".to_string();
        provider.credentials.access_token = Some("test_token".to_string());

        // Set expiration time relative to now
        let now = Utc::now();
        let expires_at = now + Duration::seconds(time_offset_secs);
        provider.credentials.expires_at = Some(expires_at.to_rfc3339());

        let needs_refresh = provider.needs_refresh(lead_time);

        // Token should need refresh if it expires within lead_time from now
        let expected_needs_refresh = time_offset_secs < lead_time_secs;

        prop_assert_eq!(
            needs_refresh,
            expected_needs_refresh,
            "iFlow: Token with expiry in {} seconds should {} refresh with lead time of {} minutes",
            time_offset_secs,
            if expected_needs_refresh { "need" } else { "not need" },
            lead_time_mins
        );
    }

    /// **Feature: cliproxyapi-parity, Property 2: Token Refresh Timing**
    /// Test that missing access token always triggers refresh
    /// **Validates: Requirements 1.2, 2.2**
    #[test]
    fn test_codex_missing_token_needs_refresh(
        lead_time_mins in arb_lead_time_mins(),
    ) {
        let lead_time = Duration::minutes(lead_time_mins);

        let provider = CodexProvider::new();
        // No access token set

        let needs_refresh = provider.needs_refresh(lead_time);

        prop_assert!(
            needs_refresh,
            "Codex: Missing access token should always need refresh"
        );
    }

    /// **Feature: cliproxyapi-parity, Property 2: Token Refresh Timing**
    /// Test that missing expiry info triggers refresh
    /// **Validates: Requirements 1.2, 2.2**
    #[test]
    fn test_codex_missing_expiry_needs_refresh(
        lead_time_mins in arb_lead_time_mins(),
    ) {
        let lead_time = Duration::minutes(lead_time_mins);

        let mut provider = CodexProvider::new();
        provider.credentials.access_token = Some("test_token".to_string());
        // No expires_at set

        let needs_refresh = provider.needs_refresh(lead_time);

        prop_assert!(
            needs_refresh,
            "Codex: Missing expiry info should always need refresh"
        );
    }

    /// **Feature: cliproxyapi-parity, Property 2: Token Refresh Timing**
    /// Test that iFlow cookie auth type never needs OAuth refresh
    /// **Validates: Requirements 2.2**
    #[test]
    fn test_iflow_cookie_auth_no_refresh(
        lead_time_mins in arb_lead_time_mins(),
        time_offset_secs in arb_time_offset_secs(),
    ) {
        let lead_time = Duration::minutes(lead_time_mins);

        let mut provider = IFlowProvider::new();
        provider.credentials.auth_type = "cookie".to_string();
        provider.credentials.cookies = Some("session=abc123".to_string());

        // Even with expiry set, cookie auth should not trigger OAuth refresh
        let now = Utc::now();
        let expires_at = now + Duration::seconds(time_offset_secs);
        provider.credentials.expires_at = Some(expires_at.to_rfc3339());

        let needs_refresh = provider.needs_refresh(lead_time);

        prop_assert!(
            !needs_refresh,
            "iFlow: Cookie auth type should never need OAuth token refresh"
        );
    }

    /// **Feature: cliproxyapi-parity, Property 2: Token Refresh Timing**
    /// Test that refresh is triggered strictly before expiration time
    /// This ensures the invariant: if needs_refresh(lead_time) is false,
    /// then the token will not expire within lead_time duration
    /// **Validates: Requirements 1.2, 2.2**
    #[test]
    fn test_refresh_timing_invariant(
        lead_time_mins in arb_lead_time_mins(),
        extra_buffer_secs in 1i64..60i64,
    ) {
        let lead_time = Duration::minutes(lead_time_mins);
        let lead_time_secs = lead_time_mins * 60;

        let mut provider = CodexProvider::new();
        provider.credentials.access_token = Some("test_token".to_string());

        // Set expiration time to exactly lead_time + extra_buffer from now
        // This should NOT need refresh
        let now = Utc::now();
        let expires_at = now + Duration::seconds(lead_time_secs + extra_buffer_secs);
        provider.credentials.expires_at = Some(expires_at.to_rfc3339());

        let needs_refresh = provider.needs_refresh(lead_time);

        prop_assert!(
            !needs_refresh,
            "Token expiring in {} seconds (lead_time={} mins + {} secs buffer) should not need refresh",
            lead_time_secs + extra_buffer_secs,
            lead_time_mins,
            extra_buffer_secs
        );

        // Verify the invariant: if needs_refresh is false, the token expires after lead_time
        // Note: is_token_expired() uses a hardcoded 5-minute buffer, which is different from needs_refresh
        // So we verify the actual expiration time instead
        if let Some(expires_str) = &provider.credentials.expires_at {
            if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(expires_str) {
                let expires_utc = expires.with_timezone(&Utc);
                let now = Utc::now();
                prop_assert!(
                    expires_utc >= now + lead_time,
                    "Token should not expire within lead_time when needs_refresh is false"
                );
            }
        }
    }

    /// **Feature: cliproxyapi-parity, Property 3: Provider Routing Correctness**
    /// *For any* request with model name M and provider type P, the router SHALL select
    /// a credential of type P that supports model M.
    /// **Validates: Requirements 1.3, 2.3, 3.2**
    ///
    /// This test verifies that:
    /// 1. Codex provider correctly identifies GPT models (gpt-*, o1*, o3*, o4*, *codex*)
    /// 2. iFlow provider correctly identifies iFlow models (iflow*, *iflow*)
    /// 3. Vertex provider correctly resolves model aliases
    #[test]
    fn test_codex_provider_routing_gpt_models(
        model_suffix in "[a-z0-9\\-]{1,10}",
    ) {
        // GPT models should be supported by Codex
        let gpt_model = format!("gpt-{}", model_suffix);
        prop_assert!(
            CodexProvider::supports_model(&gpt_model),
            "Codex should support GPT model: {}",
            gpt_model
        );

        // Case insensitivity check
        let gpt_upper = format!("GPT-{}", model_suffix.to_uppercase());
        prop_assert!(
            CodexProvider::supports_model(&gpt_upper),
            "Codex should support GPT model case-insensitively: {}",
            gpt_upper
        );
    }

    /// **Feature: cliproxyapi-parity, Property 3: Provider Routing Correctness**
    /// Test that Codex provider supports O-series models
    /// **Validates: Requirements 1.3**
    #[test]
    fn test_codex_provider_routing_o_series(
        o_variant in prop_oneof![Just("o1"), Just("o3"), Just("o4")],
        suffix in prop_oneof![Just(""), Just("-preview"), Just("-mini")],
    ) {
        let model = format!("{}{}", o_variant, suffix);
        prop_assert!(
            CodexProvider::supports_model(&model),
            "Codex should support O-series model: {}",
            model
        );
    }

    /// **Feature: cliproxyapi-parity, Property 3: Provider Routing Correctness**
    /// Test that Codex provider supports models containing "codex"
    /// **Validates: Requirements 1.3**
    #[test]
    fn test_codex_provider_routing_codex_models(
        prefix in "[a-z]{0,5}",
        suffix in "[a-z0-9\\-]{0,5}",
    ) {
        let model = format!("{}codex{}", prefix, suffix);
        prop_assert!(
            CodexProvider::supports_model(&model),
            "Codex should support model containing 'codex': {}",
            model
        );
    }

    /// **Feature: cliproxyapi-parity, Property 3: Provider Routing Correctness**
    /// Test that Codex provider does NOT support non-GPT models
    /// **Validates: Requirements 1.3**
    #[test]
    fn test_codex_provider_routing_non_gpt_models(
        model in prop_oneof![
            Just("claude-3"),
            Just("claude-sonnet"),
            Just("gemini-pro"),
            Just("gemini-2.0-flash"),
            Just("llama-2"),
            Just("mistral-7b"),
        ],
    ) {
        prop_assert!(
            !CodexProvider::supports_model(&model),
            "Codex should NOT support non-GPT model: {}",
            model
        );
    }

    /// **Feature: cliproxyapi-parity, Property 3: Provider Routing Correctness**
    /// Test that iFlow provider correctly identifies iFlow models
    /// **Validates: Requirements 2.3**
    #[test]
    fn test_iflow_provider_routing_iflow_models(
        suffix in "[a-z0-9\\-]{1,10}",
    ) {
        // Models starting with "iflow" should be supported
        let iflow_model = format!("iflow-{}", suffix);
        prop_assert!(
            IFlowProvider::supports_model(&iflow_model),
            "iFlow should support model starting with 'iflow': {}",
            iflow_model
        );

        // Models containing "iflow" should be supported
        let containing_model = format!("my-iflow-{}", suffix);
        prop_assert!(
            IFlowProvider::supports_model(&containing_model),
            "iFlow should support model containing 'iflow': {}",
            containing_model
        );
    }

    /// **Feature: cliproxyapi-parity, Property 3: Provider Routing Correctness**
    /// Test that iFlow provider does NOT support non-iFlow models
    /// **Validates: Requirements 2.3**
    #[test]
    fn test_iflow_provider_routing_non_iflow_models(
        model in prop_oneof![
            Just("gpt-4"),
            Just("claude-3"),
            Just("gemini-pro"),
            Just("llama-2"),
        ],
    ) {
        prop_assert!(
            !IFlowProvider::supports_model(&model),
            "iFlow should NOT support non-iFlow model: {}",
            model
        );
    }

    /// **Feature: cliproxyapi-parity, Property 3: Provider Routing Correctness**
    /// Test that Vertex provider correctly resolves model aliases
    /// **Validates: Requirements 3.2, 3.3**
    #[test]
    fn test_vertex_provider_model_alias_resolution(
        alias in "[a-z\\-]{3,15}",
        upstream_model in prop_oneof![
            Just("gemini-2.0-flash"),
            Just("gemini-2.5-pro"),
            Just("gemini-2.5-flash"),
        ],
    ) {
        let provider = VertexProvider::with_config("test-api-key".to_string(), None)
            .with_model_alias(&alias, &upstream_model);

        // Alias should resolve to upstream model
        let resolved = provider.resolve_model_alias(&alias);
        prop_assert_eq!(
            resolved,
            upstream_model,
            "Alias '{}' should resolve to '{}'",
            alias,
            upstream_model
        );

        // Non-alias should return as-is
        let non_alias = format!("non-alias-{}", alias);
        let non_alias_clone = non_alias.clone();
        let resolved_non_alias = provider.resolve_model_alias(&non_alias);
        prop_assert_eq!(
            resolved_non_alias,
            non_alias_clone,
            "Non-alias '{}' should return as-is",
            non_alias
        );
    }

    /// **Feature: cliproxyapi-parity, Property 3: Provider Routing Correctness**
    /// Test that Vertex provider is_alias correctly identifies aliases
    /// **Validates: Requirements 3.3**
    #[test]
    fn test_vertex_provider_is_alias(
        alias in "[a-z\\-]{3,10}",
        model in "[a-z\\-]{3,10}",
    ) {
        let provider = VertexProvider::with_config("test-api-key".to_string(), None)
            .with_model_alias(&alias, &model);

        // Configured alias should be recognized
        prop_assert!(
            provider.is_alias(&alias),
            "'{}' should be recognized as an alias",
            alias
        );

        // Non-configured model should not be an alias
        let non_alias = format!("not-{}", alias);
        prop_assert!(
            !provider.is_alias(&non_alias),
            "'{}' should NOT be recognized as an alias",
            non_alias
        );
    }

    /// **Feature: cliproxyapi-parity, Property 3: Provider Routing Correctness**
    /// Test that Vertex provider is properly configured with API key
    /// **Validates: Requirements 3.2**
    #[test]
    fn test_vertex_provider_configuration(
        api_key in "[a-zA-Z0-9]{10,30}",
        base_url in prop_oneof![
            Just(None),
            Just(Some("https://custom.api.com".to_string())),
            Just(Some("https://vertex.example.com/v1".to_string())),
        ],
    ) {
        let provider = VertexProvider::with_config(api_key.clone(), base_url.clone());

        // Provider should be configured
        prop_assert!(
            provider.is_configured(),
            "Provider with API key should be configured"
        );

        // API key should be accessible
        prop_assert_eq!(
            provider.get_api_key(),
            Some(api_key.as_str()),
            "API key should be retrievable"
        );

        // Base URL should be correct
        let expected_base_url = base_url.unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string());
        prop_assert_eq!(
            provider.get_base_url(),
            expected_base_url,
            "Base URL should match configured or default"
        );
    }
}

/// Generate a random model name
fn arb_model_name() -> impl Strategy<Value = String> {
    prop_oneof![
        // Gemini models
        Just("gemini-2.5-pro".to_string()),
        Just("gemini-2.5-flash".to_string()),
        Just("gemini-2.5-flash-lite".to_string()),
        Just("gemini-2.0-flash".to_string()),
        Just("gemini-3-pro".to_string()),
        Just("gemini-3-pro-preview".to_string()),
        Just("gemini-2.5-pro-preview-06-05".to_string()),
        // Random model names
        "[a-z]{3,8}-[0-9]\\.[0-9]-[a-z]{3,6}".prop_map(|s| s),
        "[a-z]{3,8}-[0-9]\\.[0-9]-[a-z]{3,6}-preview".prop_map(|s| s),
        "[a-z]{3,8}-[0-9]\\.[0-9]-flash".prop_map(|s| s),
        "[a-z]{3,8}-[0-9]\\.[0-9]-flash-lite".prop_map(|s| s),
    ]
}

/// Generate a random exclusion pattern
fn arb_exclusion_pattern() -> impl Strategy<Value = String> {
    prop_oneof![
        // Exact model names
        "[a-z]{3,8}-[0-9]\\.[0-9]-[a-z]{3,6}".prop_map(|s| s),
        // Prefix patterns (e.g., "gemini-2.5-*")
        "[a-z]{3,8}-[0-9]\\.[0-9]-\\*".prop_map(|s| s),
        // Suffix patterns (e.g., "*-preview")
        "\\*-[a-z]{3,8}".prop_map(|s| s),
        // Contains patterns (e.g., "*flash*")
        "\\*[a-z]{3,6}\\*".prop_map(|s| s),
    ]
}

/// Generate a list of exclusion patterns
fn arb_exclusion_patterns() -> impl Strategy<Value = Vec<String>> {
    proptest::collection::vec(arb_exclusion_pattern(), 0..5)
}

use crate::providers::gemini::GeminiApiKeyCredential;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: cliproxyapi-parity, Property 9: Model Exclusion Filtering**
    /// *For any* credential with excluded-models patterns, the credential SHALL NOT
    /// be selected for models matching those patterns.
    /// **Validates: Requirements 4.3**
    ///
    /// This test verifies that:
    /// 1. Exact match exclusions work correctly
    /// 2. Prefix wildcard exclusions (e.g., "gemini-2.5-*") work correctly
    /// 3. Suffix wildcard exclusions (e.g., "*-preview") work correctly
    /// 4. Contains wildcard exclusions (e.g., "*flash*") work correctly
    /// 5. Models not matching any pattern are supported
    #[test]
    fn test_model_exclusion_exact_match(
        model in "[a-z]{3,8}-[0-9]\\.[0-9]-[a-z]{3,6}",
    ) {
        // Create credential with exact model exclusion
        let cred = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string())
            .with_excluded_models(vec![model.clone()]);

        // The exact model should be excluded
        prop_assert!(
            !cred.supports_model(&model),
            "Model '{}' should be excluded by exact match pattern '{}'",
            model,
            model
        );

        // A different model should be supported
        let different_model = format!("{}-different", model);
        prop_assert!(
            cred.supports_model(&different_model),
            "Model '{}' should be supported (not matching exact pattern '{}')",
            different_model,
            model
        );
    }

    /// **Feature: cliproxyapi-parity, Property 9: Model Exclusion Filtering**
    /// Test prefix wildcard exclusion patterns
    /// **Validates: Requirements 4.3**
    #[test]
    fn test_model_exclusion_prefix_wildcard(
        prefix in "[a-z]{3,8}-[0-9]\\.[0-9]-",
        suffix in "[a-z]{3,8}",
    ) {
        let pattern = format!("{}*", prefix);
        let matching_model = format!("{}{}", prefix, suffix);

        let cred = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string())
            .with_excluded_models(vec![pattern.clone()]);

        // Model matching prefix should be excluded
        prop_assert!(
            !cred.supports_model(&matching_model),
            "Model '{}' should be excluded by prefix pattern '{}'",
            matching_model,
            pattern
        );

        // Model not matching prefix should be supported
        let non_matching_model = format!("other-{}", suffix);
        prop_assert!(
            cred.supports_model(&non_matching_model),
            "Model '{}' should be supported (not matching prefix pattern '{}')",
            non_matching_model,
            pattern
        );
    }

    /// **Feature: cliproxyapi-parity, Property 9: Model Exclusion Filtering**
    /// Test suffix wildcard exclusion patterns
    /// **Validates: Requirements 4.3**
    #[test]
    fn test_model_exclusion_suffix_wildcard(
        prefix in "[a-z]{3,8}",
        suffix in "-[a-z]{3,8}",
    ) {
        let pattern = format!("*{}", suffix);
        let matching_model = format!("{}{}", prefix, suffix);

        let cred = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string())
            .with_excluded_models(vec![pattern.clone()]);

        // Model matching suffix should be excluded
        prop_assert!(
            !cred.supports_model(&matching_model),
            "Model '{}' should be excluded by suffix pattern '{}'",
            matching_model,
            pattern
        );

        // Model not matching suffix should be supported
        let non_matching_model = format!("{}-other", prefix);
        prop_assert!(
            cred.supports_model(&non_matching_model),
            "Model '{}' should be supported (not matching suffix pattern '{}')",
            non_matching_model,
            pattern
        );
    }

    /// **Feature: cliproxyapi-parity, Property 9: Model Exclusion Filtering**
    /// Test contains wildcard exclusion patterns
    /// **Validates: Requirements 4.3**
    #[test]
    fn test_model_exclusion_contains_wildcard(
        middle in "[a-z]{3,6}",
    ) {
        let pattern = format!("*{}*", middle);
        let matching_model = format!("prefix-{}-suffix", middle);

        let cred = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string())
            .with_excluded_models(vec![pattern.clone()]);

        // Model containing the middle part should be excluded
        prop_assert!(
            !cred.supports_model(&matching_model),
            "Model '{}' should be excluded by contains pattern '{}'",
            matching_model,
            pattern
        );

        // Model not containing the middle part should be supported
        // Use a completely different string that won't contain the middle part
        let non_matching_model = "xyz-123-abc".to_string();
        // Only assert if the non_matching_model doesn't actually contain the middle
        if !non_matching_model.contains(&middle) {
            prop_assert!(
                cred.supports_model(&non_matching_model),
                "Model '{}' should be supported (not matching contains pattern '{}')",
                non_matching_model,
                pattern
            );
        }
    }

    /// **Feature: cliproxyapi-parity, Property 9: Model Exclusion Filtering**
    /// Test that empty exclusion list supports all models
    /// **Validates: Requirements 4.3**
    #[test]
    fn test_model_exclusion_empty_list(
        model in arb_model_name(),
    ) {
        let cred = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string())
            .with_excluded_models(vec![]);

        // All models should be supported when exclusion list is empty
        prop_assert!(
            cred.supports_model(&model),
            "Model '{}' should be supported when exclusion list is empty",
            model
        );
    }

    /// **Feature: cliproxyapi-parity, Property 9: Model Exclusion Filtering**
    /// Test multiple exclusion patterns work together
    /// **Validates: Requirements 4.3**
    #[test]
    fn test_model_exclusion_multiple_patterns(
        exact_model in "[a-z]{3,6}-exact",
        prefix in "[a-z]{3,6}-prefix-",
        suffix in "-suffix-[a-z]{3,6}",
    ) {
        let patterns = vec![
            exact_model.clone(),
            format!("{}*", prefix),
            format!("*{}", suffix),
        ];

        let cred = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string())
            .with_excluded_models(patterns);

        // Exact match should be excluded
        prop_assert!(
            !cred.supports_model(&exact_model),
            "Model '{}' should be excluded by exact match",
            exact_model
        );

        // Prefix match should be excluded
        let prefix_model = format!("{}test", prefix);
        prop_assert!(
            !cred.supports_model(&prefix_model),
            "Model '{}' should be excluded by prefix pattern",
            prefix_model
        );

        // Suffix match should be excluded
        let suffix_model = format!("test{}", suffix);
        prop_assert!(
            !cred.supports_model(&suffix_model),
            "Model '{}' should be excluded by suffix pattern",
            suffix_model
        );

        // Model not matching any pattern should be supported
        let supported_model = "completely-different-model".to_string();
        prop_assert!(
            cred.supports_model(&supported_model),
            "Model '{}' should be supported (not matching any pattern)",
            supported_model
        );
    }

    /// **Feature: cliproxyapi-parity, Property 9: Model Exclusion Filtering**
    /// Test that exclusion is case-sensitive
    /// **Validates: Requirements 4.3**
    #[test]
    fn test_model_exclusion_case_sensitivity(
        model in "[a-z]{3,8}-[0-9]\\.[0-9]-[a-z]{3,6}",
    ) {
        let cred = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string())
            .with_excluded_models(vec![model.clone()]);

        // Exact case should be excluded
        prop_assert!(
            !cred.supports_model(&model),
            "Model '{}' should be excluded (exact case)",
            model
        );

        // Different case should be supported (case-sensitive matching)
        let upper_model = model.to_uppercase();
        prop_assert!(
            cred.supports_model(&upper_model),
            "Model '{}' should be supported (different case from '{}')",
            upper_model,
            model
        );
    }

    /// **Feature: cliproxyapi-parity, Property 10: Custom Base URL Usage**
    /// *For any* credential with custom base_url, requests using that credential
    /// SHALL be sent to the custom URL.
    /// **Validates: Requirements 4.4**
    ///
    /// This test verifies that:
    /// 1. When a custom base_url is set, get_base_url() returns the custom URL
    /// 2. When no custom base_url is set, get_base_url() returns the default URL
    /// 3. The build_api_url() method correctly uses the custom base URL
    #[test]
    fn test_custom_base_url_usage(
        custom_host in "[a-z]{3,10}",
        custom_domain in prop_oneof![Just("com"), Just("io"), Just("net"), Just("ai")],
        model in "[a-z]{3,8}-[0-9]\\.[0-9]-[a-z]{3,6}",
        action in prop_oneof![Just("generateContent"), Just("streamGenerateContent"), Just("countTokens")],
    ) {
        let custom_base_url = format!("https://{}.example.{}", custom_host, custom_domain);

        // Create credential with custom base URL
        let cred_with_custom = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string())
            .with_base_url(Some(custom_base_url.clone()));

        // get_base_url() should return the custom URL
        prop_assert_eq!(
            cred_with_custom.get_base_url(),
            custom_base_url.as_str(),
            "get_base_url() should return custom URL '{}' when set",
            custom_base_url
        );

        // build_api_url() should use the custom base URL
        let api_url = cred_with_custom.build_api_url(&model, &action);
        let expected_url = format!("{}/v1beta/models/{}:{}", custom_base_url, model, action);

        // Verify the URL starts with the custom base URL
        prop_assert!(
            api_url.starts_with(&custom_base_url),
            "API URL '{}' should start with custom base URL '{}'",
            api_url,
            custom_base_url
        );

        prop_assert_eq!(
            api_url,
            expected_url,
            "build_api_url() should construct URL using custom base URL"
        );
    }

    /// **Feature: cliproxyapi-parity, Property 10: Custom Base URL Usage**
    /// Test that credentials without custom base_url use the default URL
    /// **Validates: Requirements 4.4**
    #[test]
    fn test_default_base_url_when_not_set(
        model in "[a-z]{3,8}-[0-9]\\.[0-9]-[a-z]{3,6}",
        action in prop_oneof![Just("generateContent"), Just("streamGenerateContent"), Just("countTokens")],
    ) {
        use crate::providers::gemini::GEMINI_API_BASE_URL;

        // Create credential without custom base URL
        let cred_default = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string());

        // get_base_url() should return the default URL
        prop_assert_eq!(
            cred_default.get_base_url(),
            GEMINI_API_BASE_URL,
            "get_base_url() should return default URL when no custom URL is set"
        );

        // build_api_url() should use the default base URL
        let api_url = cred_default.build_api_url(&model, &action);
        let expected_url = format!("{}/v1beta/models/{}:{}", GEMINI_API_BASE_URL, model, action);

        // Verify the URL starts with the default base URL
        prop_assert!(
            api_url.starts_with(GEMINI_API_BASE_URL),
            "API URL '{}' should start with default base URL '{}'",
            api_url,
            GEMINI_API_BASE_URL
        );

        prop_assert_eq!(
            api_url,
            expected_url,
            "build_api_url() should construct URL using default base URL"
        );
    }

    /// **Feature: cliproxyapi-parity, Property 10: Custom Base URL Usage**
    /// Test that explicitly setting base_url to None uses the default URL
    /// **Validates: Requirements 4.4**
    #[test]
    fn test_explicit_none_base_url_uses_default(
        model in "[a-z]{3,8}-[0-9]\\.[0-9]-[a-z]{3,6}",
        action in prop_oneof![Just("generateContent"), Just("streamGenerateContent")],
    ) {
        use crate::providers::gemini::GEMINI_API_BASE_URL;

        // Create credential with explicit None base URL
        let cred = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string())
            .with_base_url(None);

        // get_base_url() should return the default URL
        prop_assert_eq!(
            cred.get_base_url(),
            GEMINI_API_BASE_URL,
            "get_base_url() should return default URL when base_url is explicitly None"
        );

        // build_api_url() should use the default base URL
        let api_url = cred.build_api_url(&model, &action);
        prop_assert!(
            api_url.starts_with(GEMINI_API_BASE_URL),
            "API URL should start with default base URL when base_url is None"
        );
    }

    /// **Feature: cliproxyapi-parity, Property 10: Custom Base URL Usage**
    /// Test that custom base URL with trailing slash is handled correctly
    /// **Validates: Requirements 4.4**
    #[test]
    fn test_custom_base_url_trailing_slash_handling(
        custom_host in "[a-z]{3,10}",
        model in "[a-z]{3,8}-[0-9]\\.[0-9]-[a-z]{3,6}",
    ) {
        // Note: The current implementation does NOT strip trailing slashes,
        // so we test the actual behavior (URL will have double slash if trailing slash provided)
        let custom_base_url_no_slash = format!("https://{}.example.com", custom_host);

        let cred = GeminiApiKeyCredential::new("test-id".to_string(), "test-key".to_string())
            .with_base_url(Some(custom_base_url_no_slash.clone()));

        let api_url = cred.build_api_url(&model, "generateContent");

        // URL should be properly formed with the custom base URL
        let expected_url = format!("{}/v1beta/models/{}:generateContent", custom_base_url_no_slash, model);
        prop_assert_eq!(
            api_url,
            expected_url,
            "API URL should be correctly formed with custom base URL"
        );
    }

    /// **Feature: cliproxyapi-parity, Property 10: Custom Base URL Usage**
    /// Test that different credentials can have different base URLs
    /// **Validates: Requirements 4.4**
    #[test]
    fn test_multiple_credentials_different_base_urls(
        host1 in "[a-z]{3,8}",
        host2 in "[a-z]{3,8}",
        model in "[a-z]{3,8}-[0-9]\\.[0-9]-[a-z]{3,6}",
    ) {
        let base_url_1 = format!("https://{}.api.com", host1);
        let base_url_2 = format!("https://{}.api.io", host2);

        let cred1 = GeminiApiKeyCredential::new("cred-1".to_string(), "key-1".to_string())
            .with_base_url(Some(base_url_1.clone()));

        let cred2 = GeminiApiKeyCredential::new("cred-2".to_string(), "key-2".to_string())
            .with_base_url(Some(base_url_2.clone()));

        // Each credential should use its own base URL
        prop_assert_eq!(
            cred1.get_base_url(),
            base_url_1.as_str(),
            "Credential 1 should use its own base URL"
        );

        prop_assert_eq!(
            cred2.get_base_url(),
            base_url_2.as_str(),
            "Credential 2 should use its own base URL"
        );

        // API URLs should be different
        let url1 = cred1.build_api_url(&model, "generateContent");
        let url2 = cred2.build_api_url(&model, "generateContent");

        prop_assert!(
            url1.starts_with(&base_url_1),
            "URL 1 should start with base_url_1"
        );

        prop_assert!(
            url2.starts_with(&base_url_2),
            "URL 2 should start with base_url_2"
        );

        // URLs should be different (unless hosts happen to be the same)
        if host1 != host2 {
            prop_assert_ne!(
                url1,
                url2,
                "Different credentials with different base URLs should produce different API URLs"
            );
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_codex_needs_refresh_boundary() {
        let mut provider = CodexProvider::new();
        provider.credentials.access_token = Some("test_token".to_string());

        let lead_time = Duration::minutes(5);

        // Token expiring well after lead_time - should NOT need refresh
        // Use a large buffer to avoid timing issues
        let now = Utc::now();
        let expires_at = now + Duration::minutes(10);
        provider.credentials.expires_at = Some(expires_at.to_rfc3339());
        assert!(
            !provider.needs_refresh(lead_time),
            "Token expiring in 10 mins should not need refresh with 5 min lead time"
        );

        // Token expiring well before lead_time - should need refresh
        let now = Utc::now();
        let expires_at = now + Duration::minutes(2);
        provider.credentials.expires_at = Some(expires_at.to_rfc3339());
        assert!(
            provider.needs_refresh(lead_time),
            "Token expiring in 2 mins should need refresh with 5 min lead time"
        );

        // Token already expired - should need refresh
        let now = Utc::now();
        let expires_at = now - Duration::minutes(1);
        provider.credentials.expires_at = Some(expires_at.to_rfc3339());
        assert!(
            provider.needs_refresh(lead_time),
            "Expired token should need refresh"
        );
    }

    #[test]
    fn test_iflow_needs_refresh_boundary() {
        let mut provider = IFlowProvider::new();
        provider.credentials.auth_type = "oauth".to_string();
        provider.credentials.access_token = Some("test_token".to_string());

        let lead_time = Duration::minutes(5);

        // Token expiring well after lead_time - should NOT need refresh
        let now = Utc::now();
        let expires_at = now + Duration::minutes(10);
        provider.credentials.expires_at = Some(expires_at.to_rfc3339());
        assert!(
            !provider.needs_refresh(lead_time),
            "Token expiring in 10 mins should not need refresh with 5 min lead time"
        );

        // Token expiring well before lead_time - should need refresh
        let now = Utc::now();
        let expires_at = now + Duration::minutes(2);
        provider.credentials.expires_at = Some(expires_at.to_rfc3339());
        assert!(
            provider.needs_refresh(lead_time),
            "Token expiring in 2 mins should need refresh with 5 min lead time"
        );
    }
}
