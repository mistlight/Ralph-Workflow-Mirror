# Data Pipeline: [Brief title]

> **How to use this template:** This template is for data processing pipeline development. Fill in the goal and acceptance criteria below to guide the AI agent.

## Goal
[One-line description of the data pipeline or ETL process]

## Questions to Consider

**Data Flow:**
* What is the data source (database, API, file system, message queue)?
* What transformations need to be applied to the data?
* What is the destination (data warehouse, analytics, another system)?
* What is the expected data volume and velocity?

**Reliability:**
* How should the pipeline handle failures (retry, skip, alert)?
* Should the pipeline support idempotent processing?
* Are there any data quality checks that need to be performed?
* Should the pipeline support backfilling historical data?

**Performance:**
* What are the latency requirements (real-time, batch, streaming)?
* Should the pipeline process data in parallel?
* Are there any memory constraints for large datasets?
* Should intermediate results be cached or persisted?

**Monitoring:**
* What metrics should be tracked (records processed, error rate, latency)?
* How should alerts be configured for pipeline failures?
* Should there be visibility into data quality metrics?
* Is audit logging required for compliance?

## Acceptance Checks
* [Pipeline successfully reads from data source]
* [Data transformations produce correct output format]
* [Pipeline handles errors gracefully without data loss]
* [Processing completes within performance requirements]
* [Monitoring and logging capture pipeline state]
* [Data quality validations detect anomalies]
* [Pipeline can be restarted from last checkpoint]
* [Documentation includes data schema and flow diagrams]

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

**Feature Implementation Best Practices:**
- Start with the simplest working solution, optimize only if needed
- Prefer standard library solutions over external dependencies
- Add logging at key points (entry/exit of major functions, errors)
- Use types to make invalid states unrepresentable
- Document non-obvious design decisions in comments
- Consider the API ergonomics - is it pleasant to use?

**Security Considerations:**
- Validate all user input at system boundaries
- Sanitize data before display (prevent injection attacks)
- Use parameterized queries to prevent SQL injection
- Follow the principle of least privilege for permissions
- Never log sensitive data (passwords, tokens, PII)
- Consider encryption for sensitive data at rest and in transit

**EXAMPLE:**
```markdown
# Data Pipeline: Daily Sales Aggregation

## Goal
Create a pipeline that aggregates daily sales data into summary statistics.

## Questions to Consider
**Data Flow:**
- Source: Transaction database
- Transform: Group by product, calculate sum/avg/count
- Destination: Analytics warehouse

**Reliability:**
- Retry failed database operations 3 times
- Skip records with validation errors (log them)
- Support idempotent reprocessing

**Performance:**
- Batch size: 10,000 records
- Complete within 1 hour for 1M records

**Acceptance Checks:**
- [Processes all transactions for the day]
- [Aggregates match manual calculations]
- [Handles duplicate runs without double-counting]
```
