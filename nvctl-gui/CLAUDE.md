# nvctl-gui - Claude Code Configuration

> Iced-based GUI for nvctl GPU control tool

## Quick Commands

```bash
make gui            # Run GUI (release mode, smooth animations)
make gui-dev        # Run GUI (debug mode)
make gui-check      # Check GUI (fmt + clippy)
make gui-test       # Run GUI tests
make gui-build      # Build release binary
```

## Git Rules

**FORBIDDEN:** `git push`, `git commit`, `git rebase`, `git push --force`
**ALLOWED:** `git status`, `git diff`, `git log`, `git add --dry-run`

---

## Architecture: The Elm Architecture

nvctl-gui follows The Elm Architecture (TEA). All code must adhere to this pattern.

### Core Pattern
```
State -> View -> User Interaction -> Message -> Update -> State
```

### File Organization (Feature-Based)
```
src/
├── main.rs              # Entry point only
├── app.rs               # Application struct, update, view
├── message.rs           # Global Message enum
├── theme.rs             # Custom dark theme, colors
├── screens/             # Feature modules (state + update + view together)
├── widgets/             # Custom canvas widgets
└── services/            # Backend integration (nvctl-lib)
```

**CRITICAL:** Do NOT split state, update, and view into separate modules.
Keep related code together in feature-based modules.

---

## Iced Patterns

### IMPORTANT: Message Hierarchy

```rust
// CORRECT: Hierarchical messages with mapping
enum Message {
    Dashboard(DashboardMessage),
    FanControl(FanControlMessage),
    // ...
}

// In view:
screen.view().map(Message::FanControl)
```

### IMPORTANT: Canvas Widget Caching

```rust
// CORRECT: Always use cache
struct MyWidget {
    cache: canvas::Cache,
}

impl canvas::Program<Message> for MyWidget {
    fn draw(&self, state: &State, ...) -> Vec<Geometry> {
        state.cache.draw(renderer, bounds.size(), |frame| {
            // Drawing code
        })
    }
}

// Clear cache ONLY when data changes
fn update(&mut self, msg: Message) {
    match msg {
        Message::DataChanged(_) => self.cache.clear(),
    }
}
```

### IMPORTANT: State Design

```rust
// CORRECT: Make impossible states impossible
enum AppState {
    Loading,
    Ready(Data),
    Error(String),
}

// WRONG: Ambiguous state
struct App {
    is_loading: bool,
    data: Option<Data>,
    error: Option<String>,
}
```

### IMPORTANT: Async Pattern

```rust
// CORRECT: Result-based messages
Message::LoadData => {
    Command::perform(
        async { load_data().await },
        |result| Message::DataLoaded(result.map_err(|e| e.to_string()))
    )
}

Message::DataLoaded(Ok(data)) => { /* handle success */ }
Message::DataLoaded(Err(e)) => { /* handle error */ }
```

---

## Critical Rules

### IMPORTANT: No Panics
```rust
// CORRECT: Return Result or handle gracefully
fn get_data(&self) -> Result<Data, Error> { ... }

// WRONG: Never unwrap in library code
fn get_data(&self) -> Data { self.inner.unwrap() }  // NO!
```

### IMPORTANT: Color Constants
```rust
// CORRECT: Use theme colors
use crate::theme::colors;
let color = colors::temp_color(temperature);

// WRONG: Hardcoded colors scattered in code
let color = Color::from_rgb(0.0, 0.8, 1.0);
```

### IMPORTANT: Widget Composition
```rust
// CORRECT: Compose with helper functions
fn view(&self) -> Element<Message> {
    column![
        self.view_header(),
        self.view_content(),
        self.view_footer(),
    ].into()
}

// WRONG: Monolithic view function
```

---

## Testing

### Unit Tests (Business Logic)
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_fan_curve() {
        let curve = FanCurve::default();
        assert_eq!(curve.speed_at(50), 40);
    }
}
```

### Widget Tests (with iced_test)
```rust
#[test]
fn test_button_click() {
    let app = App::new();
    let ui = Simulator::with(app.view());
    let (_, messages) = ui.click("Apply");
    assert!(messages.contains(&Message::Apply));
}
```

---

## Code Style

### Derives for Messages
```rust
#[derive(Debug, Clone)]  // Always derive these
enum Message { ... }
```

### Derives for State
```rust
#[derive(Debug, Default)]
struct AppState { ... }
```

### Widget Builder Pattern
```rust
// Chain builder methods
button("Apply")
    .on_press(Message::Apply)
    .style(theme::Button::Primary)
    .padding(10)
```

---

## File References

- Entry point: `src/main.rs:1`
- Application: `src/app.rs:20` (App struct)
- Messages: `src/message.rs:1` (Message enum)
- Theme: `src/theme.rs:1` (colors, custom theme)
- Temp Gauge Widget: `src/widgets/temp_gauge.rs:10`
- Fan Curve Editor: `src/widgets/fan_curve.rs:15`

---

## Quality Checklist

Before completing any task:
- [ ] `make gui-check` passes (fmt + clippy)
- [ ] `make gui-test` passes
- [ ] No `.unwrap()` or `.expect()` in library code
- [ ] Canvas widgets use `cache`
- [ ] New widgets have tests
- [ ] Messages are hierarchical
- [ ] State uses enums for mutually exclusive states

---

## Agent Configuration

### Proactive Agents (use automatically)
| Agent | Trigger |
|-------|---------|
| `code-reviewer` | After writing significant Iced code |
| `code-simplifier` | After complex widget implementations |
| `type-design-analyzer` | When adding new Message or State types |

### On-Demand Agents
| Agent | Use Case |
|-------|----------|
| `Explore` | Understanding widget patterns |
| `Plan` | Planning new screens or features |
