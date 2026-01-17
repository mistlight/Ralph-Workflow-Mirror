# Database Migration: [Migration Description]

> **How to use this template:** Use this template when making schema changes, data migrations, or database refactoring. Focus on zero-downtime migrations and rollback strategies.

## Goal
[EXAMPLE: Add user preference table with default values and migrate existing user settings]

## Questions to Consider

**Impact Analysis:**
- What tables/columns are being added, modified, or removed?
- How will this affect existing queries and application code?
- Will this require application code changes?
- Can this be done online with zero downtime?
- What is the data volume and how long will migration take?

**Backwards Compatibility:**
- Will old application code work with the new schema?
- Can you deploy in phases (schema change, then code change)?
- Is there a feature flag to enable/disable new functionality?
- What is the rollback strategy if something goes wrong?

**Data Integrity:**
- How will you handle data validation during migration?
- What happens to data that doesn't validate?
- Are there foreign key constraints to consider?
- How will you handle orphaned records?

## Migration Plan

**Phase 1: Preparation**
1. [Create migration script with proper up/down migrations]
2. [Test migration on staging database with production-like data]
3. [Plan rollback strategy]
4. [Set up monitoring for migration performance]

**Phase 2: Schema Migration (if zero-downtime required)**
[Describe steps for zero-downtime migration]
[EXAMPLE:
1. Add new column as nullable
2. Deploy code that writes to both old and new columns
3. Backfill data for existing rows
4. Deploy code that reads from new column
5. Remove old column (in separate migration)
]

**Phase 3: Data Migration**
[Describe data migration if applicable]
[EXAMPLE: Migrate user settings from JSON column to new preference table]

**Phase 4: Verification**
1. [Verify data integrity after migration]
2. [Run performance tests to ensure no degradation]
3. [Monitor production metrics closely after deployment]

## Migration Specification

**Database:**
[PostgreSQL, MySQL, SQLite, etc.]

**Changes:**
[Describe the schema changes in detail]
[EXAMPLE:
- Create table user_preferences (id, user_id, theme, notifications, created_at)
- Add foreign key constraint user_preferences.user_id -> users.id
- Add index on user_preferences.user_id for performance
]

**Data Volume:**
[Estimate the number of rows to be affected]
[EXAMPLE: ~50,000 users to migrate, estimated 5 minutes]

**Rollback Plan:**
[Describe how to rollback if something goes wrong]
[EXAMPLE:
1. Drop new user_preferences table
2. No rollback needed for data as old JSON column still exists
]

## Acceptance
- [Migration runs successfully on production database]
- [No data loss or corruption]
- [Application continues to work during migration (zero-downtime)]
- [Performance is not degraded]
- [Rollback tested and documented]
- [Migration script is idempotent (can be re-run safely)]
- [Database indexes and constraints properly configured]

## Code Quality Specifications

Write clean, maintainable code:
- Single responsibility: one reason to change per function/class
- Small units: functions < 30 lines, classes < 300 lines
- Clear names that reveal intent
- Early returns; minimize nesting depth
- Explicit error handling; no silent failures
- No magic numbers; extract constants
- DRY: extract duplicated logic
- Validate at boundaries; trust internal data
- Test behavior, not implementation

**Database Migration Best Practices:**
- **Idempotent:** Migrations should be re-runnable without errors
- **Reversible:** Always write down migration for rollback
- **Test First:** Test migrations on staging with production-like data
- **Batch Large Changes:** Process data in batches for large tables
- **Add Indexes:** Add indexes before queries that need them, drop after data migration
- **Use Transactions:** Wrap migrations in transactions when possible
- **Lock Management:** Be aware of table locks and their duration
- **Zero Downtime:** Plan for backwards-compatible changes
- **Monitor:** Watch database performance during migration
- **Document:** Document the migration strategy and rollback plan

**Zero-Downtime Migration Pattern:**
1. Add new column/table (nullable or with defaults)
2. Deploy code that writes to both old and new
3. Backfill data in batches
4. Deploy code that reads from new
5. Remove old column/table (separate migration)

**Testing Strategy for Migrations:**
- Test migration script on development database
- Test migration on staging with production data copy
- Test rollback script on staging
- Add integration tests for new schema
- Run data validation queries after migration
- Monitor query performance after migration
- Have rollback plan ready in case of issues
