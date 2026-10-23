//! Offset pagination with a hard cap.

use serde::Deserialize;

/// Default number of items returned per page.
pub const DEFAULT_PAGE_LIMIT: u32 = 50;
/// Hard cap for `limit`; larger requests are clamped.
pub const MAX_PAGE_LIMIT: u32 = 200;

/// Raw query parameters shared by list endpoints.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct PageParams {
    /// 1-based page number.
    pub page: Option<u32>,
    /// Requested page size.
    pub limit: Option<u32>,
}

/// Resolved pagination values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    /// 1-based page number.
    pub page: u32,
    /// Effective page size (always `1..=MAX_PAGE_LIMIT`).
    pub limit: u32,
    /// SQL offset.
    pub offset: i64,
}

impl PageRequest {
    /// Resolve parameters, clamping out-of-range values instead of failing so a
    /// hostile or buggy client cannot make the server allocate unbounded pages.
    pub fn resolve(params: PageParams) -> Self {
        let page = params.page.unwrap_or(1).max(1);
        let limit = params.limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
        Self {
            page,
            limit,
            offset: i64::from(page.saturating_sub(1)) * i64::from(limit),
        }
    }

    /// Convenience for callers that already have optional values.
    pub fn from_optional(page: Option<u32>, limit: Option<u32>) -> Self {
        Self::resolve(PageParams { page, limit })
    }

    /// Total number of pages for a result set of `total` rows (at least 1).
    pub fn total_pages(total: i64) -> u32 {
        if total <= 0 {
            return 1;
        }
        let limit = i64::from(DEFAULT_PAGE_LIMIT).max(1);
        // `total_pages` is only meaningful together with a concrete limit; this
        // helper exists for dashboards that use the default limit.
        u32::try_from(total.div_ceil(limit)).unwrap_or(u32::MAX)
    }

    /// Total number of pages for a concrete limit.
    pub fn pages_for(total: i64, limit: u32) -> u32 {
        if total <= 0 {
            return 1;
        }
        let limit = i64::from(limit.max(1));
        u32::try_from(total.div_ceil(limit)).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_used() {
        let page = PageRequest::resolve(PageParams::default());
        assert_eq!(page.page, 1);
        assert_eq!(page.limit, DEFAULT_PAGE_LIMIT);
        assert_eq!(page.offset, 0);
    }

    #[test]
    fn limit_is_capped() {
        let page = PageRequest::resolve(PageParams {
            page: Some(1),
            limit: Some(1_000),
        });
        assert_eq!(page.limit, MAX_PAGE_LIMIT);
        assert_eq!(page.offset, 0);
    }

    #[test]
    fn zero_values_are_normalized() {
        let page = PageRequest::resolve(PageParams {
            page: Some(0),
            limit: Some(0),
        });
        assert_eq!(page.page, 1);
        assert_eq!(page.limit, 1);
        assert_eq!(page.offset, 0);
    }

    #[test]
    fn offsets_are_computed() {
        let page = PageRequest::resolve(PageParams {
            page: Some(3),
            limit: Some(20),
        });
        assert_eq!(page.offset, 40);
    }

    #[test]
    fn extreme_pages_do_not_overflow() {
        let page = PageRequest::resolve(PageParams {
            page: Some(u32::MAX),
            limit: Some(MAX_PAGE_LIMIT),
        });
        assert_eq!(page.offset, 429_496_729_600);
    }

    #[test]
    fn page_counts() {
        assert_eq!(PageRequest::pages_for(0, 50), 1);
        assert_eq!(PageRequest::pages_for(1, 50), 1);
        assert_eq!(PageRequest::pages_for(50, 50), 1);
        assert_eq!(PageRequest::pages_for(51, 50), 2);
        assert_eq!(PageRequest::pages_for(200, 200), 1);
        assert_eq!(PageRequest::total_pages(120), 3);
    }
}
