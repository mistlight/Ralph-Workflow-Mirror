# Web API: [Brief title]

> **How to use this template:** This template is for REST/HTTP API development. Fill in the goal and acceptance criteria below to guide the AI agent.

## Goal
[One-line description of the API endpoint or service]

## Questions to Consider

**API Design:**
* What is the HTTP method (GET, POST, PUT, DELETE, PATCH)?
* What is the resource path structure?
* What are the required and optional request parameters?
* What should the response format be (JSON, XML, etc.)?

**Error Handling:**
* What are the possible error conditions?
* What HTTP status codes should be returned?
* Should error responses follow a specific format (RFC 7807, etc.)?
* How should validation errors be communicated?

**Security:**
* Does the endpoint require authentication/authorization?
* What are the rate limiting requirements?
* Are there any input validation concerns (SQL injection, XSS, etc.)?
* Should CORS be configured for cross-origin requests?

**Performance:**
* What is the expected request volume?
* Should caching be implemented (ETag, Cache-Control headers)?
* Are there any database query optimization needs?
* Should pagination be supported for list endpoints?

## Acceptance Checks
* [Endpoint responds with correct HTTP status codes]
* [Request body is validated with clear error messages]
* [Response format matches API specification]
* [Authentication/authorization works as expected]
* [Error responses include helpful messages]
* [Rate limiting prevents abuse]
* [Logging captures request/response for debugging]
* [Documentation includes request/response examples]

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
- Sanitize data before display (prevent XSS)
- Use parameterized queries to prevent SQL injection
- Follow the principle of least privilege for permissions
- Never log sensitive data (passwords, tokens, PII)
- Consider rate limiting for public-facing APIs

**EXAMPLE:**
```markdown
# Web API: User Profile Endpoint

## Goal
Create GET /api/users/{id} endpoint that returns user profile information.

## Questions to Consider
**API Design:**
- HTTP GET method
- Path parameter: user ID
- Response: JSON with user details

**Security:**
- Require authentication token
- Only return own profile or admin can view any
- Rate limit: 100 requests per minute

**Acceptance Checks:**
- [Returns 200 with user data for valid ID]
- [Returns 401 for missing authentication]
- [Returns 403 for unauthorized access]
- [Returns 404 for non-existent user]
```
