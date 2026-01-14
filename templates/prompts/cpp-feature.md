# [Feature Name]

> **How to use this template:** This C++-specific template is for implementing new features with modern C++ best practices. The sections below help you think through the design and provide clear acceptance criteria for the AI agent.

## Goal
[Clear description of what you want to build]

**Tips for a good goal:**
- "Add a generic container class with move semantics and iterator support"
- "Implement async I/O with ASIO and coroutines for C++20"
- "Create a thread-safe memory pool with RAII and exception safety"

## Questions to Consider
Before implementing, think through:

**C++-Specific Design:**
- What memory management approach? (RAII, smart pointers, value semantics)
- Are there lifetime considerations? (owners vs observers, dangling pointers)
- Should this use templates? (generic programming, type erasure)
- What error handling approach? (exceptions, error codes, std::expected, std::optional)
- Are there move semantics opportunities? (avoid copies, use std::move)
- What about const correctness? (const methods, const references, constexpr)

**Edge Cases:**
- What happens with invalid input? (use error codes or std::optional, not undefined behavior)
- What about null pointers? (use std::optional, references, or gsl::not_null)
- Are there race conditions? (thread safety, atomics, locks)
- How do you handle integer overflow? (use checked arithmetic, saturating arithmetic)
- What about exception safety? (strong, basic, no-throw guarantees)

**Impact:**
- Are there performance implications? (cache locality, allocations, hot paths)
- What about ABI compatibility? (pimpl idiom, versioning)
- Are external dependencies involved? (libraries, package managers like vcpkg/conan)
- What about backwards compatibility? (API design, deprecation)

**Security & Error Handling:**
- Are there potential security vulnerabilities? (buffer overflow, use-after-free, double-free)
- How should errors be handled and communicated? (exceptions, error codes, std::expected)
- What sensitive data is involved? (zero on free, secure erase)
- Are there resource leaks? (use RAII, smart pointers, containers)

**Compatibility:**
- Which C++ standard? (C++11, C++14, C++17, C++20, C++23)
- Which compilers? (GCC, Clang, MSVC version support)
- What about platform differences? (Windows vs POSIX)
- Will this require breaking changes to existing APIs?

## Acceptance Checks
- [Specific, testable condition 1]
- [Specific, testable condition 2]
- [Specific, testable condition 3]

**Tips for acceptance criteria:**
- Make them specific and measurable
- Focus on behavior, not implementation
- Include error cases and edge cases

## Constraints
- [Any limitations or requirements]
- [Performance requirements, if applicable]
- [Compatibility notes]

## Context
[Relevant background information]
[Why this change is needed]
[Impact on existing code]

## Implementation Notes (Optional)
[Architecture considerations]
[Potential approaches]
[Files/modules likely affected]

## C++ Best Practices

**Modern C++ Standards:**
- Use C++17 or C++20 when possible (latest compiler support)
- Use `auto` for type deduction when type is obvious (avoid auto for readability when helpful)
- Use `auto&&` in generic code (forwarding references, perfect forwarding)
- Use `decltype(auto)` for perfect return type deduction
- Use `constexpr` for compile-time constants and functions
- Use `consteval` for functions that must execute at compile time (C++20)
- Use `constinit` for constant initialization (C++20)
- Use `if constexpr` for compile-time branching (C++17)

**RAII & Resource Management:**
- Use RAII for all resources (files, sockets, locks, memory)
- Use smart pointers: `std::unique_ptr<T>` for exclusive ownership
- Use `std::shared_ptr<T>` for shared ownership (reference counting)
- Use `std::weak_ptr<T>` to break cycles in shared_ptr
- Use `std::make_unique<T>()` and `std::make_shared<T>()` for allocation
- Avoid `new` and `delete` (use smart pointers or containers)
- Use containers (std::vector, std::string, std::map) instead of C arrays
- Use `std::span<T>` (C++20) or `gsl::span` for view into contiguous memory

**Move Semantics:**
- Use `std::move` to cast to rvalue reference (enable move semantics)
- Use `std::forward<T>` for perfect forwarding (universal references)
- Use move semantics to avoid copies (pass by value, then std::move)
- Return by value (RVO/move elision is efficient)
- Use `noexcept` for move operations (enables optimizations)
- Be aware of self-move assignment (check `this != &other`)

**Templates & Generic Programming:**
- Use templates for generic algorithms and containers
- Use `template<typename T>` for type parameters
- Use `template<auto N>` for non-type template parameters (C++17)
- Use `typename T::type` or `T::type` for dependent types
- Use `requires` clauses for concepts (C++20)
- Use concepts to constrain template parameters (C++20)
- Use `auto` template parameters for simplified syntax (C++17)
- Use fold expressions for variadic templates (C++17)
- Use `if constexpr` for compile-time branching in templates (C++17)
- Use `std::type_traits` for type traits and compile-time reflection

**Error Handling:**
- Prefer exceptions for error handling (use error codes only when needed)
- Use `throw` to signal exceptional conditions (not for control flow)
- Use `noexcept` to mark functions that don't throw (enables optimizations)
- Use `try`/`catch` for exception handling
- Use `std::error_code` and `std::system_error` for system errors
- Use `std::expected<T, E>` (C++23) or `tl::expected` for expected/error results
- Use `std::optional<T>` (C++17) for optional values (may or may not have a value)
- Use `std::variant<T, U>` (C++17) for sum types (discriminated unions)
- Use `std::any` (C++17) for type-safe container of any type
- Use exception specifications (`noexcept`, `throw()`) sparingly

**Const Correctness:**
- Mark methods `const` when they don't modify the object
- Pass by `const T&` for read-only references (avoid copies)
- Use `constexpr` for compile-time constants
- Use `const` on local variables when possible (prevent reassignment)
- Use `const` iterators for read-only iteration (cbegin, cend)
- Use `const` pointers and references when ownership isn't transferred

**Concurrency & Thread Safety:**
- Use `std::thread` for threads (join with join() or detach with detach())
- Use `std::mutex` for mutual exclusion (lock with std::lock_guard, std::unique_lock)
- Use `std::shared_mutex` for multiple readers, single writer (C++17)
- Use `std::atomic<T>` for lock-free synchronization (atomic operations)
- Use `std::condition_variable` for waiting on conditions
- Use `std::future<T>` and `std::promise<T>` for async results
- Use `std::async` for asynchronous task execution
- Use `std::scoped_lock<T>` for multiple locks (deadlock avoidance, C++17)
- Use `std::lock_guard<T>` for RAII mutex locking
- Use `std::unique_lock<T>` for flexible locking (manual lock/unlock)
- Use `std::call_once` and `std::once_flag` for one-time initialization
- Use thread-local storage with `thread_local` keyword
- Use `std::jthread` (C++20) for automatically joining threads

**STL Containers & Algorithms:**
- Use `std::vector<T>` for dynamic arrays (default container)
- Use `std::string` for text (avoid C strings)
- Use `std::map<K, V>` or `std::unordered_map<K, V>` for associative arrays
- Use `std::set<T>` or `std::unordered_set<T>` for unique elements
- Use `std::deque<T>` for double-ended queues
- Use `std::list<T>` only when you need frequent insertion in the middle
- Use `std::queue<T>` or `std::stack<T>` for adapters
- Use `std::priority_queue<T>` for heap-based priority queue
- Use `std::span<T>` (C++20) for non-owning view into contiguous memory
- Use STL algorithms (`std::sort`, `std::find`, `std::transform`) instead of manual loops
- Use lambda expressions with `[]` capture for local functions
- Use `std::function<R(Args...)>` for type-erased callables

**Smart Pointers & Ownership:**
- Use `std::unique_ptr<T>` for exclusive ownership (cannot be copied, only moved)
- Use `std::make_unique<T>()` (C++14) for exception-safe allocation
- Use `std::shared_ptr<T>` for shared ownership (reference counted)
- Use `std::make_shared<T>()` for allocation in single block (ref count + object)
- Use `std::weak_ptr<T>` to observe shared_ptr (prevents cycles)
- Use custom deleters with smart pointers (`std::unique_ptr<T, Deleter>`)
- Use `std::enable_shared_from_this<T>` to get shared_ptr from this

**Coroutines (C++20):**
- Use `co_await` for awaiting coroutines (asynchronous operations)
- Use `co_yield` for yielding values from coroutines (generators)
- Use `co_return` for returning from coroutines
- Use coroutine types (task, generator, lazy) for async/await patterns

**Ranges (C++20):**
- Use ranges for composable algorithms (`std::ranges::sort`, `std::ranges::filter`)
- Use range views (`std::views::filter`, `std::views::transform`)
- Use range adaptors for lazy evaluation (pipeline of operations)

**Code Organization:**
- Use headers (`.h`, `.hpp`) for declarations
- Use source files (`.cpp`) for definitions
- Use include guards (`#pragma once` or `#ifndef`...`#endif`)
- Use namespaces to organize code (avoid `using namespace std;` in headers)
- Use inline namespaces for versioning
- Use anonymous namespaces for file-local linkage (instead of `static`)
- Use translation units for physical separation of code

**Testing:**
- Write unit tests alongside code (test.cpp or tests/test_*.cpp)
- Use Catch2, GTest, or doctest for unit testing frameworks
- Test normal paths, edge cases, and error paths
- Use test fixtures for shared setup/teardown
- Use mock objects for dependencies (Google Mock, trompeloeil)
- Use static analysis tools (clang-tidy, cppcheck)
- Use sanitizers (ASan, MSan, TSan, UBSan) for detecting bugs
- Use Valgrind for memory leak detection (memcheck)

**Build & Tooling:**
- Use CMake for cross-platform build configuration
- Use package managers (vcpkg, Conan) for dependencies
- Use C++ compiler warnings (`-Wall -Wextra -Wpedantic` for GCC/Clang)
- Use static analysis (clang-tidy, cppcheck, PVS-Studio)
- Use formatters (clang-format) for consistent code style
- Use linters (clang-tidy) for code quality checks
- Use sanitizers (`-fsanitize=address,undefined`) for detecting bugs
- Use link-time optimization (`-flto`) for performance
- Use profile-guided optimization (PGO) for hot paths

**Performance:**
- Profile before optimizing (use perf, VTune, profiler)
- Use `std::move` to avoid copies (enable move semantics)
- Use `emplace_back` instead of `push_back` to construct in place
- Use reserve() on vectors to avoid reallocations (preallocate capacity)
- Use small object optimization (SBO) for small types (std::string has SBO)
- Use `[[likely]]` and `[[unlikely]]` attributes for branch hints (C++20)
- Use `std::array<T, N>` for stack-allocated fixed-size arrays
- Use `std::string_view` (C++17) for non-owning string references (avoid copies)
- Use `std::span<T>` (C++20) for non-owning array views (avoid copies)
- Be aware of cache locality and prefetching (data-oriented design)

## Security Considerations
- Validate all user input at system boundaries (bounds checking, sanitization)
- Use std::string instead of C strings (avoid buffer overflows)
- Use std::vector, std::array instead of C arrays (bounds checking)
- Use std::span<T> (C++20) or gsl::span for bounds-checked array views
- Use `gsl::not_null<T>` for non-null pointers (enforce at compile time)
- Use sanitizers (ASan, MSan, TSan, UBSan) to detect memory bugs
- Use `std::erase` and `std::erase_if` (C++20) for safe container modification
- Use `std::atomic` for thread-safe operations (avoid data races)
- Use `volatile` only for memory-mapped I/O (not for thread safety)
- Use `std::memcpy` and `std::memset` instead of C equivalents
- Use `std::fill` and `std::copy` instead of manual loops
- Use `std::equal` instead of `memcmp` for comparisons
