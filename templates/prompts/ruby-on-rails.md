# [Feature Name]

> **How to use this template:** This Ruby on Rails-specific template is for implementing new features with Rails best practices. The sections below help you think through the design and provide clear acceptance criteria for the AI agent.

## Goal
[Clear description of what you want to build]

**Tips for a good goal:**
- "Add user authentication with Devise and OmniAuth for OAuth providers"
- "Implement a RESTful API with JSON responses for a Post resource"
- "Create a background job for processing payments with Stripe webhooks"

## Questions to Consider
Before implementing, think through:

**Rails-Specific Design:**
- Which Rails components are needed? (models, controllers, views, jobs, mailers, channels)
- Are there associations between models? (has_many, belongs_to, has_one, has_and_belongs_to_many)
- Should this use a background job? (ActiveJob, Sidekiq, Resque)
- What validations are needed at the model level? (presence, uniqueness, format, length)
- Are there callbacks needed? (before_save, after_create, around_update)
- How will you handle database transactions? (wrapping multi-step operations)

**Edge Cases:**
- What happens with invalid input? (model validations, strong parameters)
- What about nil values? (use `presence`, `&.`, `||=`, compact/compact_blank)
- Are there race conditions? (optimistic locking with `lock_version`)
- How do you handle concurrent requests? (database transactions, locks)
- What about database constraints? (unique indexes, foreign keys, check constraints)

**Impact:**
- Are there performance implications? (N+1 queries, missing indexes, eager loading)
- What about the database schema? (migrations, indexes, foreign keys)
- Are external dependencies involved? (gems, APIs, third-party services)
- What about caching? (Redis, Memcached, Russian Doll caching)

**Security & Error Handling:**
- Are there potential security vulnerabilities? (SQL injection, XSS, CSRF, mass assignment)
- How should errors be handled and communicated? (rescue_from, custom error pages)
- What sensitive data is involved? (encrypt at rest, filter from logs)
- Are there rate limiting or authorization concerns? (CanCanCan, Pundit, Rolify)
- How will you handle file uploads? (ActiveStorage, Shrine, Paperclip, CarrierWave)

**Compatibility:**
- Will this require database migrations? (backward compatibility, rollback plans)
- Are there breaking API changes? (versioning, deprecation warnings)
- Will this require changes to existing models or controllers?

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
[Files/gems likely affected]

## Rails Best Practices

**Models (ActiveRecord):**
- Use model validations (presence, uniqueness, format, length, inclusion)
- Use scopes for query reuse (`scope :published, -> { where(published: true) }`)
- Use callbacks sparingly (prefer business logic in service objects or models)
- Use associations correctly (has_many, belongs_to, has_one, has_and_belongs_to_many)
- Use delegated methods for cleaner APIs (`delegate :name, to: :user`)
- Use enum for discrete states (`enum status: [:draft, :published, :archived]`)
- Use `touch: true` on associations for updated_at timestamps
- Avoid business logic in controllers (use service objects or form objects)

**Controllers (ActionController):**
- Follow RESTful conventions (index, show, new, create, edit, update, destroy)
- Use strong parameters (permit expected attributes, require top-level key)
- Keep actions thin (3-5 lines if possible)
- Use before_action for shared logic (authentication, authorization, loading resources)
- Use `head :no_content` instead of `render json: {}` for empty responses
- Use `redirect_to` after non-GET requests (PRG pattern)
- Use rescue_from for error handling (standard error pages)
- Use `turbo_frame` or `turbo_stream` for Hotwire/Turbo responses

**Views (ActionView):**
- Use partials for reusable components (shared/_header.html.erb)
- Use locals to pass data to partials (`render partial: 'post', locals: { post: @post }`)
- Use content_for for layout sections (yield :title)
- Use helpers for view logic (avoid complex Ruby in views)
- Use ERB or Haml for templating (avoid putting logic in templates)
- Use Turbo Streams for dynamic updates without full page reloads
- Use Stimulus controllers for JavaScript behavior (avoid inline scripts)

**Routing:**
- Use resource routing (`resources :posts` instead of manual routes)
- Use member and collection routes for additional actions (`member :publish` vs `collection :search`)
- Use shallow nesting for deeply nested routes (`shallow do resources :comments end`)
- Use namespace for admin/API routes (`namespace :admin do resources :posts end`)
- Use concerns for shared route logic (`concerns :paginatable`)

**Migrations:**
- Use reversible migrations (up/down or change method)
- Use `change_column_default` and `change_column_null` for reversible changes
- Use `add_reference` with `index: true` and `foreign_key: true`
- Use `add_index` for frequently queried columns
- Use `add_check_constraint` for data integrity
- Keep migrations fast (avoid data migrations in schema migrations)
- Use `schema.rb` for version control (not structure.sql unless needed)

**Background Jobs (ActiveJob):**
- Use background jobs for long-running tasks (email, processing, webhooks)
- Use Sidekiq or Resque for job queuing (configure retries, queues)
- Use `set(queue: :critical)` for job prioritization
- Use `perform_later` for enqueuing, `perform_now` for synchronous execution
- Use job arguments with GlobalID (ActiveRecord objects, not IDs)
- Use `retry_on` and `discard_on` for error handling
- Use ActiveJob proxies for method invocation on records (`User.find(1).invitation_job.perform_later`)

**Testing (RSpec / Minitest):**
- Write tests alongside code (TDD or test-after)
- Use factory_bot for test data (avoid hardcoded fixtures)
- Use Faker for realistic test data (names, emails, addresses)
- Use capybara for feature/integration tests (fill in forms, click buttons)
- Use shoulda-matchers for common matchers (validate_presence_of, belong_to)
- Use SimpleCov or SimpleCov-json for coverage reporting
- Test edge cases and error paths (not just happy paths)
- Use transactional fixtures for test isolation
- Use VCR or WebMock for stubbing HTTP requests

**Security:**
- Use strong parameters to prevent mass assignment (`params.require(:post).permit(:title)`)
- Use `has_secure_password` for password hashing (bcrypt)
- Use `protect_from_forgery with: :exception` for CSRF protection
- Use `before_action :authenticate_user!` for authentication (Devise)
- Use Pundit or CanCanCan for authorization (policy objects)
- Use `render json: { error: '...' }, status: :forbidden` for API errors
- Use param sanitization for user input (strip, squish, sanitize)
- Use SQL parameterization (ActiveRecord handles this automatically)
- Use `content_security_policy` for XSS protection
- Use `force_ssl` for SSL enforcement (though SSL termination is preferred)

**Performance:**
- Avoid N+1 queries (use `includes`, `joins`, `preload` for eager loading)
- Use counter caches for association counts (add column, use `counter_cache: true`)
- Use database indexes for frequently queried columns
- Use pagination for large datasets (Kaminari, will_paginate, pagy)
- Use fragment caching for expensive view rendering (`cache @post do ... end`)
- Use Russian Doll caching for nested cache dependencies
- Use background jobs for slow operations (email, processing, webhooks)
- Use `pluck` for selecting single columns (avoid instantiating models)
- Use `find_each` or `find_in_batches` for large result sets
- Use bullet gem to detect N+1 queries in development

**API Development:**
- Use `respond_to` for format negotiation (JSON, XML, HTML)
- Use `ActiveModel::Serializers` or `blueprinter` for JSON serialization
- Use versioned API routes (`namespace :api do namespace :v1 do ... end end`)
- Use pagination headers (Link header, RFC 5988)
- Use CORS for cross-origin requests (`rack-cors` gem)
- Use API tokens or JWT for authentication (Devise tokens, doorkeeper)
- Use `render json: @post, status: :created` for proper status codes
- Use `head :no_content` for DELETE requests

**Code Organization:**
- Use service objects for business logic (app/services/create_order.rb)
- Use form objects for multi-model forms (app/forms/order_form.rb)
- Use view components for reusable UI (app/components/button_component.rb)
- Use concerns for shared model/controller code (app/models/concerns/...)
- Use decorators or presenters for view formatting (draper, active_decorator)
- Use query objects for complex queries (app/queries/published_posts.rb)
- Use policy objects for authorization (app/policies/post_policy.rb)
- Use value objects for domain concepts (app/values/money.rb)

## Security Considerations
- Validate all user input with strong parameters (whitelist expected attributes)
- Sanitize data before display (use `sanitize` helper, avoid `html_safe`)
- Use parameterized queries (ActiveRecord handles this automatically)
- Use `has_secure_password` for password hashing (bcrypt with salt)
- Use `protect_from_forgery` for CSRF protection (authenticity tokens)
- Use `force_ssl` for SSL enforcement (redirect HTTP to HTTPS)
- Filter sensitive params from logs (`config.filter_parameters += [:password]`)
- Use `attr_encrypted` or `attr_vault` for encrypted attributes
- Use Rack::Attack for rate limiting and IP-based blocking
- Use Brakeman or bundler-audit for security vulnerability scanning
- Keep gems up to date (use `bundle outdated`, Gemfile.lock for reproducibility)
