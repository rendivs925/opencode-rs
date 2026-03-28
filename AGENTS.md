# AI Agent Guidelines

This document provides universal guidelines for AI agents working in any codebase. These rules apply to all languages (Rust, TypeScript, Python, Go, etc.). When language-specific examples are needed, Rust is used.

## Core Principles

1. **Never break existing functionality** - Test changes before submitting
2. **Prefer existing patterns** - Follow what's already in the codebase
3. **Keep changes focused** - One logical change per PR
4. **Validate all input** - Never trust external data without validation
5. **Handle errors properly** - Never silently swallow errors

---

# Code Quality Rules

## Always Avoid

| Rule                                    | Example (Rust)                | Reason                                        |
| --------------------------------------- | ----------------------------- | --------------------------------------------- |
| No `unwrap()` / `expect()` / `panic!()` | `value.unwrap()`              | Unhandled panics crash the application        |
| No `.clone()` without justification     | `data.clone()`                | Memory overhead; often indicates design issue |
| No mutable global state                 | `static mut COUNTER: u32 = 0` | Creates unpredictable behavior                |
| No hardcoded secrets                    | `let api_key = "secret"`      | Security risk; use environment variables      |
| No debug prints in production           | `println!("{:?}", data)`      | Performance and security concern              |

## Size Limits

| Metric                | Limit     | Action if Exceeded       |
| --------------------- | --------- | ------------------------ |
| Files                 | ≤ 300 LOC | Split by responsibility  |
| Functions             | ≤ 60 LOC  | Extract sub-functions    |
| Match arms / branches | ≤ 10      | Extract helper functions |

## Layering

| Layer             | Location                                               |
| ----------------- | ------------------------------------------------------ |
| Data access / SQL | `repo/`, `repository/`, `data/`, `db/`                 |
| Business logic    | `services/`, `domain/`, `application/`, `usecases/`    |
| HTTP/RPC          | `controllers/`, `handlers/`, `api/`, `infrastructure/` |
| UI components     | `components/`, `views/`, `ui/`, `presentation/`        |

---

# Type System Rules

## Use Types, Not Primitives

```rust
// CORRECT - typed enum
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserRole { Admin, Manager, User }

// WRONG - string comparison
if role == "admin" { ... }
```

```typescript
// CORRECT - TypeScript
enum UserRole { Admin, Manager, User }

// WRONG - string comparison
if (role === "admin") { ... }
```

## Use Structs/Interfaces for Structured Data

```rust
// CORRECT - typed struct
#[derive(Serialize, Deserialize)]
pub struct CreateUserInput {
    pub email: String,
    pub name: String,
}

// WRONG - loose object
let user: serde_json::Value = ...;
```

```typescript
// CORRECT - typed interface
interface CreateUserInput {
    email: string;
    name: string;
}

// WRONG - loose object
const user: Record<string, unknown> = { ... };
```

## Use Built-in Serialization

```rust
// CORRECT - automatic serialization with serde
#[derive(Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status { Active, Inactive }

// WRONG - manual parsing
fn parse_status(s: &str) -> Status {
    match s {
        "ACTIVE" => Status::Active,
        ...
    }
}
```

```typescript
// CORRECT - TypeScript enum
enum Status {
  Active = "ACTIVE",
  Inactive = "INACTIVE",
}

// WRONG - manual string mapping
function parseStatus(s: string): Status {
  return s === "ACTIVE" ? Status.Active : Status.Inactive
}
```

## Avoid Unnecessary DTOs

```rust
// WRONG - duplicate type
pub struct UserDto { /* same fields as User */ }

// CORRECT - reuse the type directly
pub async fn get_user() -> Result<User, Error>
```

```typescript
// WRONG - duplicate interface
interface UserDto {
  email: string
  name: string
}

// CORRECT - reuse the type directly
async function getUser(): Promise<User>
```

### When DTOs ARE Allowed

1. API boundary with different schema
2. Aggregated/computed response types
3. Security filtering (hiding internal fields)

---

# Error Handling

- Use typed errors (`Result<T, E>` in Rust, custom error classes in other languages)
- Log errors **once** at the boundary
- Never expose internal error details to users
- Return meaningful error messages

```rust
// CORRECT - Rust
match save_user(&user).await {
    Ok(_) => Ok(Response::success()),
    Err(e) => {
        error!("Failed to save user: {:?}", e);
        Err(Error::UserSave("Unable to save user".into()))
    }
}
```

```typescript
// CORRECT - TypeScript
try {
  await saveUser(user)
} catch (error) {
  logger.error("Failed to save user", { error, userId: user.id })
  throw new UserSaveError("Unable to save user. Please try again.")
}
```

```python
# CORRECT - Python
try:
    save_user(user)
except DatabaseError as e:
    logger.error(f"Failed to save user: {e}")
    raise UserSaveError("Unable to save user") from e
```

---

# Validation

- Validate all user input at system boundaries
- Use validation libraries (validator, garde, zod, pydantic, etc.)
- Return clear, actionable error messages

```rust
// CORRECT - Rust with validator
pub fn create_user(input: CreateUserInput) -> Result<User, ValidationError> {
    validated = UserSchema.validate(&input)?;
    Ok(User::from(validated))
}
```

```typescript
// CORRECT - TypeScript with Zod
function createUser(input: unknown): Result<User, ValidationError> {
  const parsed = userSchema.safeParse(input)
  if (!parsed.success) {
    return Err(new ValidationError(parsed.error.errors))
  }
  return Ok(parsed.data)
}
```

---

# Duplicate Code

## Never Duplicate

- Enum or struct definitions
- Repeated match blocks / conditionals
- Helper functions
- Validation logic
- Type definitions that already exist

## Always Extract

```rust
// WRONG - duplicated logic
if user.role == Role::Admin { ... } // in 10 places

// CORRECT - shared function
pub fn is_admin(user: &User) -> bool {
    matches!(user.role, Role::Admin)
}
```

```typescript
// WRONG - duplicated logic
if (user.role === "admin") { ... } // in 10 places

// CORRECT - shared function
function isAdmin(user: User): boolean {
    return user.role === "admin";
}
```

---

# Testing

- Write tests for new functionality
- Follow existing test patterns in the project
- Keep tests focused and readable

---

# CI Gates

Run the appropriate commands for your project:

```bash
# Format check
cargo fmt --all -- --check    # Rust
npm run format -- --check      # TypeScript/JavaScript
black --check .                # Python

# Lint check (warnings are errors)
cargo clippy --all-targets --all-features -- -D warnings  # Rust
npm run lint                   # TypeScript/JavaScript
ruff check .                   # Python

# Type check
cargo check --all-targets       # Rust
npm run typecheck              # TypeScript
mypy .                         # Python

# Tests
cargo test --workspace         # Rust
npm test                       # TypeScript/JavaScript
pytest                         # Python
```

---

# PR Checklist

Before submitting:

- [ ] No `unwrap()` / `expect()` / `panic!()` in production paths (Rust)
- [ ] No `throw` without proper handling in production paths
- [ ] No `.clone()` without justification (Rust)
- [ ] Files ≤ 300 LOC, functions ≤ 60 LOC
- [ ] No duplicate enum/struct definitions
- [ ] No unnecessary DTOs or mapping layers
- [ ] Enums used for closed sets instead of strings
- [ ] Typed structs used instead of loose objects
- [ ] Serialization handled by built-in libraries (serde, zod, pydantic, etc.)
- [ ] Errors handled properly with single log point
- [ ] All user input validated
- [ ] Format check passes
- [ ] Lint passes
- [ ] Type check passes
- [ ] Tests pass
