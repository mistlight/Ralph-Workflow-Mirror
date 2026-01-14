# [Feature Name]

> **How to use this template:** This TypeScript/Node.js-specific template is for implementing new features with TypeScript best practices. The sections below help you think through the design and provide clear acceptance criteria for the AI agent.

## Goal
[Clear description of what you want to build]

**Tips for a good goal:**
- "Add user authentication with JWT using Express and TypeScript"
- "Implement real-time updates with WebSocket and React hooks"
- "Create a generic data repository with TypeORM and Zod validation"

## Questions to Consider
Before implementing, think through:

**TypeScript-Specific Design:**
- What types are needed? (interfaces, types, enums, generics)
- Are there union types or discriminated unions for state?
- Should this use `async`/`await`? (Promise-based APIs)
- What error handling approach? (try/catch, Result types, never-throw functions)
- How will you validate runtime types? (Zod, Yup, io-ts, class-validator)

**Edge Cases:**
- What happens with invalid input? (use Zod schemas, validate before using)
- What about `null`/`undefined`? (use strictNullChecks, handle undefined explicitly)
- Are there race conditions or timing issues? (async operations, event ordering)
- How do you handle missing properties? (optional chaining `?.`, nullish coalescing `??`)

**Impact:**
- Are there performance implications? (bundle size, runtime performance)
- What about the build process? (tsc, esbuild, swc, webpack, vite)
- Are external dependencies involved? (npm packages, version compatibility)
- What about type coverage? (avoid `any`, use `unknown` for untyped data)

**Security & Error Handling:**
- Are there potential security vulnerabilities? (XSS, injection, prototype pollution)
- How should errors be handled and communicated? (throw, return Result types, custom error classes)
- What sensitive data is involved? (don't log secrets, sanitize for output)
- Are there rate limiting or resource exhaustion concerns? (DDoS protection, backpressure)

**Compatibility:**
- Will this require breaking changes to existing APIs? (SemVer considerations)
- Are backward compatibility requirements? (versioning, feature flags)
- Will this require changes to dependent packages or consumers?

**Framework-Specific:**
- For **Node.js**: Express, Fastify, Koa, NestJS?
- For **React**: hooks, context, state management (Redux, Zustand, Jotai)?
- For **build tools**: TypeScript compiler, bundler, testing setup?

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

## TypeScript Best Practices

**Type Safety:**
- Enable `strict` mode in `tsconfig.json` (strictNullChecks, noImplicitAny)
- Avoid `any`; use `unknown` for untyped data and narrow the type
- Use discriminated unions for state with type guards (`is`, `in`, `typeof`, `instanceof`)
- Use `as const` for readonly tuples and literal types
- Use template literal types for string patterns
- Use branded types for domain-specific values (e.g., `type UserId = string & { readonly __brand: unique symbol }`)

**Generics:**
- Use generics for reusable components and utilities
- Constrain generics with `extends` for better type inference
- Use conditional types for type-level logic
- Use `infer` for extracting types from other types
- Prefer type parameters over concrete types when appropriate

**Error Handling:**
- Use `try`/`catch` for async operations (or `.catch()` for Promises)
- Consider never-throw functions that return `Result<T, E>` (neverthrow, fp-ts)
- Create custom error classes that extend `Error` (preserve stack traces)
- Type error responses from APIs (discriminated unions for success/error)
- Use error boundaries in React for catching component errors

**Async/Await:**
- Prefer `async`/`await` over Promise chains for readability
- Handle Promise rejections (use `.catch()` or try/catch)
- Use `Promise.all` for concurrent operations (use `Promise.allSettled` to tolerate failures)
- Use `Promise.race` for timeout patterns
- Be aware of unhandled promise rejections (attach `.catch()` or use global handlers)

**Runtime Type Validation:**
- Use Zod, Yup, or io-ts to validate data at runtime boundaries (API responses, environment variables)
- Validate user input before using it (schemas for request bodies, query params)
- Use `.parse()` and throw or handle validation errors
- Use `.safeParse()` for non-throwing validation
- Infer TypeScript types from Zod schemas (z.infer<typeof schema>)

**Code Organization:**
- Organize by feature, not by layer (e.g., `auth/`, not `models/`, `controllers/`)
- Use `index.ts` files to re-export public APIs (barrel exports)
- Use path aliases (`@/`, `@/lib`) for clean imports
- Keep modules focused (single responsibility)
- Use barrel exports (`export * from './module'`) for aggregating exports

**React-Specific (if applicable):**
- Use functional components with hooks (avoid class components)
- Use TypeScript for props (define interfaces for component props)
- Use `React.FC` sparingly (direct function types are more flexible)
- Use generic types for reusable components (`<T extends {}>(props: Props<T>)`)
- Use proper types for context (`createContext<ContextType | null>(null)`)
- Use custom hooks for reusable logic (extract logic from components)

**Node.js-Specific (if applicable):**
- Use `ts-node` or `tsx` for development (JIT compilation)
- Use `tsconfig.json` with appropriate `module` and `target` settings
- Use `@types/*` packages for type definitions (DefinitelyTyped)
- Use `import` statements (ES modules) over `require` (CommonJS)
- Use `package.json` `exports` field for package entry points
- Handle `process.env` with type-safe environment variable loaders (dotenv, zod)

**Testing:**
- Write tests alongside code in `*.test.ts` or `*.spec.ts` files
- Use Jest, Vitest, or node:test for unit and integration tests
- Use testing libraries for React (React Testing Library, @testing-library/user-event)
- Mock external dependencies (vi.mock in Vitest, jest.mock)
- Test error paths (unhappy paths), not just success paths
- Use type assertions sparingly in tests (`as` is a code smell)

**Build & Tooling:**
- Use `tsc --noEmit` for type checking without emitting JS
- Use `esbuild`, `swc`, or `vite` for faster builds (instead of `tsc` for emit)
- Enable `skipLibCheck` if you have large `node_modules` with type errors
- Use `ts-node` or `tsx` for running TypeScript directly
- Use `ts-patch` or `ts-alias` for path alias resolution
- Use `tsup` or `tsup` for building libraries

**Dependencies:**
- Minimize dependencies (prefer built-in APIs and lightweight libraries)
- Keep dependencies up to date (`npm outdated`, `npm audit`)
- Use `package.json` `overrides` or `resolutions` to dedupe dependencies
- Prefer ES modules over CommonJS when possible
- Use `npm` workspaces or monorepo tools (Turborepo, Nx) for large projects

**Documentation:**
- Document public APIs with TSDoc comments (`/** ... */`)
- Include examples in documentation
- Document generic type parameters with `@template` tags
- Document thrown errors with `@throws` tags
- Document overloaded function signatures with separate comments

## Security Considerations
- Validate all user input at system boundaries (use Zod schemas)
- Sanitize data before display to prevent XSS (use DOMPurify, escape user input)
- Use parameterized queries to prevent SQL injection (use query builders, ORMs)
- Use Content Security Policy (CSP) headers to prevent XSS
- Be aware of prototype pollution (avoid `object[key]` patterns, sanitize objects)
- Use `helmet` or similar for Express security headers
- Don't log sensitive data (sanitize secrets, passwords, tokens before logging)
- Use `crypto.timingSafeEqual` for secret comparisons (avoid timing attacks)
- Use `bcrypt` or `argon2` for password hashing (don't implement your own crypto)
- Keep dependencies up to date and audit for vulnerabilities (`npm audit`)
