# API Integration: [Third-Party Service Name]

> **How to use this template:** Use this template when integrating with third-party APIs, external services, or microservices. Focus on error handling, retry logic, and proper API design.

## Goal
[EXAMPLE: Integrate Stripe API for payment processing with proper error handling and webhook validation]

## Questions to Consider

**API Design:**
- What is the API's purpose and functionality? What endpoints do you need?
- What are the authentication requirements? (API key, OAuth, JWT)
- What are the rate limits and how will you handle them?
- What is the request/response format? (JSON, XML, protobuf)
- Are there webhooks or callbacks to handle?

**Error Handling:**
- What are the possible error responses? (4xx, 5xx, network errors)
- How will you handle transient failures? (retry logic, exponential backoff)
- How will you handle authentication failures?
- How will you handle rate limiting?
- What is the fallback strategy if the API is down?

**Data Management:**
- What data needs to be stored vs. fetched on-demand?
- How will you cache API responses to reduce calls?
- How will you handle pagination for large datasets?
- Are there data transformation requirements?

## Integration Plan

**Phase 1: Client Setup**
[Describe API client setup]
[EXAMPLE: Create Stripe client with API key from environment variables]
[EXAMPLE: Configure timeout and retry settings]

**Phase 2: Core Functionality**
[Describe the main integration points]
[EXAMPLE: Implement payment intent creation, confirmation, and webhook handling]
[EXAMPLE: Add database schema to store payment status and transaction IDs]

**Phase 3: Error Handling and Resilience**
1. [Implement retry logic with exponential backoff]
2. [Add circuit breaker to prevent cascading failures]
3. [Add logging for API calls and errors]
4. [Add monitoring for API health]

**Phase 4: Testing**
1. [Add unit tests with mocked API responses]
2. [Add integration tests with test API credentials]
3. [Add error injection tests for resilience]

## API Specification

**Base URL:**
[EXAMPLE: https://api.stripe.com/v1]

**Authentication:**
[EXAMPLE: Bearer token (API key) in Authorization header]

**Key Endpoints:**
[List the main endpoints you'll use]
[EXAMPLE:
- POST /v1/payment_intents - Create payment intent
- POST /v1/payment_intents/{id}/confirm - Confirm payment
- GET /v1/payment_intents/{id} - Retrieve payment intent
]

**Rate Limits:**
[Document rate limits and how you'll handle them]
[EXAMPLE: 100 requests per second - implement token bucket rate limiter]

## Acceptance
- [Integration works correctly with the actual API]
- [Proper error handling for all error scenarios]
- [Retry logic with exponential backoff implemented]
- [Webhooks validated for authenticity]
- [Comprehensive test coverage including edge cases]
- [Documentation updated with integration details]
- [Monitoring and alerting configured]

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

**API Integration Best Practices:**
- **Configuration:** Store API keys in environment variables, never in code
- **Timeouts:** Set appropriate timeouts for API calls (connect, read)
- **Retries:** Implement exponential backoff with jitter for retries
- **Circuit Breaker:** Prevent cascading failures when API is down
- **Logging:** Log API requests, responses, and errors (without sensitive data)
- **Monitoring:** Track API health, latency, error rates
- **Caching:** Cache responses when appropriate to reduce API calls
- **Pagination:** Handle large datasets with proper pagination
- **Idempotency:** Design operations to be idempotent when possible
- **Webhooks:** Verify webhook signatures for authenticity

**Error Handling Strategy:**
- **4xx Errors:** Client errors - don't retry, log and surface to user
- **5xx Errors:** Server errors - retry with exponential backoff
- **Network Errors:** Transient failures - retry with exponential backoff
- **Timeout Errors:** Retry with increased timeout
- **Rate Limiting (429):** Respect Retry-After header or back off exponentially
- **Authentication Errors (401):** Refresh token or fail permanently

**Testing Strategy for API Integrations:**
- Mock external API calls in unit tests
- Test success scenarios with realistic responses
- Test error scenarios (4xx, 5xx, network errors, timeouts)
- Test retry logic with simulated failures
- Add integration tests with test environment
- Test webhook handling if applicable
- Add contract tests if API has a schema
