# Performance Optimization: [Component/Feature Name]

> **How to use this template:** Use this template when optimizing code for better performance, reducing latency, improving throughput, or reducing resource usage. Focus on measurable improvements with proper benchmarking.

## Goal
[EXAMPLE: Reduce API response time from 500ms to under 100ms for the user search endpoint]

## Questions to Consider

**Measurement:**
- What are the current performance metrics? (response time, throughput, memory usage, CPU usage)
- How will you measure improvement? (benchmarking tools, profiling, load testing)
- What is the target performance goal? Is it based on requirements or user expectations?

**Investigation:**
- Where is the bottleneck? (database queries, network calls, algorithmic complexity, memory allocation)
- Is this a hot path or critical path that affects user experience?
- Are there N+1 query problems or inefficient data structures?
- Is there unnecessary work being done (repeated computations, redundant processing)?

**Trade-offs:**
- What are you trading off for performance? (readability, maintainability, memory, development time)
- Is the optimization premature or does it solve a real problem?
- Will this optimization scale as load increases?

## Performance Profile

**Current State:**
[EXAMPLE: Current API endpoint takes 500ms average response time with 100 concurrent users]

**Target State:**
[EXAMPLE: Target: <100ms response time with 1000 concurrent users]

**Bottleneck Analysis:**
[Identify the specific bottleneck - e.g., database queries, N+1 problem, inefficient algorithm]
[EXAMPLE: Identified N+1 query problem in user search - making 1 query for users + N queries for each user's profile]

## Optimization Plan

**Phase 1: Profiling and Measurement**
1. [Set up benchmarking - e.g., criteron.rs for Rust, benchmark.js for Node.js]
2. [Profile the code to identify hot paths - e.g., flamegraphs, CPU profilers]
3. [Establish baseline metrics for comparison]

**Phase 2: Implementation**
[Describe the specific optimization approach]
[EXAMPLE: Implement eager loading with JOIN to fetch user profiles in a single query]
[EXAMPLE: Add database indexes on frequently queried columns]

**Phase 3: Validation**
1. [Verify performance improvement meets target]
2. [Ensure correctness is maintained - all tests pass]
3. [Run load tests to verify improvement under stress]

## Acceptance
- [Performance target achieved and verified with benchmarks]
- [Regression test suite passes - no functional changes]
- [Load tests demonstrate improvement under realistic conditions]
- [Code remains readable and maintainable]
- [Documentation updated if optimization changes API or behavior]

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

**Performance Optimization Best Practices:**
- Profile before optimizing - measure don't guess
- Optimize the critical path, not micro-optimizations
- Use appropriate data structures (e.g., HashMap vs Vec for lookups)
- Consider caching for expensive operations
- Use lazy evaluation where appropriate
- Batch operations to reduce overhead
- Consider parallel processing for CPU-bound tasks
- Use connection pooling for database/network operations
- Implement pagination for large datasets
- Use compression for network transfers

**Testing Strategy for Performance:**
- Add benchmark tests that capture before/after metrics
- Add unit tests to verify correctness is maintained
- Add load tests to verify behavior under stress
- Add regression tests to prevent performance degradation
- Monitor in production to validate real-world improvement
