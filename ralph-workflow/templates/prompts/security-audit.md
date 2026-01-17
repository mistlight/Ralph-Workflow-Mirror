# Security Audit: [Component/System Name]

> **How to use this template:** Use this template when conducting security reviews, fixing vulnerabilities, or implementing security controls. Focus on defense in depth and following security best practices.

## Goal
[EXAMPLE: Fix SQL injection vulnerability in user search endpoint and implement parameterized queries]

## Questions to Consider

**Threat Modeling:**
- What are the assets to protect? (user data, system resources, API keys)
- Who are the potential attackers? (authenticated users, public internet, insiders)
- What are the attack vectors? (injection, XSS, CSRF, authentication bypass, privilege escalation)
- What is the impact if security fails? (data breach, service disruption, financial loss)

**Vulnerability Analysis:**
- Is user input properly validated and sanitized?
- Are queries parameterized to prevent injection attacks?
- Is authentication and authorization properly implemented?
- Are sensitive data properly encrypted at rest and in transit?
- Are there race conditions or time-of-check-time-of-use (TOCTOU) vulnerabilities?
- Is there proper error handling that doesn't leak sensitive information?

**Compliance:**
- Are there regulatory requirements? (GDPR, HIPAA, PCI-DSS)
- Are there industry standards to follow? (OWASP Top 10, CIS benchmarks)
- Are there organizational security policies to comply with?

## Security Assessment

**Vulnerability Description:**
[Describe the security issue clearly]
[EXAMPLE: User search endpoint accepts raw SQL in search parameter, allowing SQL injection]

**Attack Scenario:**
[Describe how an attacker could exploit this]
[EXAMPLE: Attacker could input "'; DROP TABLE users; --" to delete the users table]

**Impact Assessment:**
- **Severity:** [Critical/High/Medium/Low]
- **Data at Risk:** [What data could be accessed/exposed/modified]
- **Business Impact:** [Financial, reputational, operational impact]

## Remediation Plan

**Phase 1: Immediate Fix**
1. [Implement the security fix - e.g., parameterized queries]
2. [Add input validation and sanitization]
3. [Add security tests to prevent regression]

**Phase 2: Defense in Depth**
[Describe additional security layers]
[EXAMPLE: Implement web application firewall (WAF) rules]
[EXAMPLE: Add rate limiting to prevent brute force attacks]

**Phase 3: Monitoring and Detection**
1. [Add logging for security events]
2. [Set up alerts for suspicious activity]
3. [Implement incident response procedures]

## Acceptance
- [Vulnerability is fixed and verified with security tests]
- [No regressions in functionality]
- [Security test suite passes]
- [Code review completed by security team if required]
- [Documentation updated with security considerations]
- [Incident response plan updated if needed]

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

**Security Best Practices:**
- **Input Validation:** Validate, sanitize, and encode all user input
- **Injection Prevention:** Use parameterized queries, prepared statements, ORMs
- **Authentication:** Use strong password policies, multi-factor authentication
- **Authorization:** Implement principle of least privilege, role-based access control
- **Encryption:** Encrypt sensitive data at rest and in transit (TLS, AES)
- **Session Management:** Use secure, HTTP-only, same-site cookies
- **Error Handling:** Don't leak sensitive information in error messages
- **Logging:** Log security events without logging sensitive data
- **Dependencies:** Keep dependencies updated, scan for vulnerabilities
- **Secrets Management:** Never hardcode secrets, use secure vaults

**OWASP Top 10 Coverage:**
- [A01: Broken Access Control] - Verify proper authorization checks
- [A02: Cryptographic Failures] - Verify encryption of sensitive data
- [A03: Injection] - Verify parameterized queries and input sanitization
- [A04: Insecure Design] - Verify threat modeling was conducted
- [A05: Security Misconfiguration] - Verify secure defaults and configurations
- [A06: Vulnerable and Outdated Components] - Verify dependencies are updated
- [A07: Identification and Authentication Failures] - Verify strong authentication
- [A08: Software and Data Integrity Failures] - Verify code signing and integrity checks
- [A09: Security Logging and Monitoring Failures] - Verify security event logging
- [A10: Server-Side Request Forgery (SSRF)] - Verify validation of URLs and redirects

**Testing Strategy for Security:**
- Add unit tests for security critical paths
- Add integration tests for authentication/authorization
- Add fuzzing tests for input validation
- Add dependency scanning for vulnerable packages
- Add static analysis (SAST) to codebase
- Conduct manual security review
- Consider penetration testing for critical systems
