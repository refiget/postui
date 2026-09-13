# Examples

## Button gallery

`button_gallery` renders response toolbar button variants and records focus and
activation events.

```bash
cargo run --example button_gallery
```

Use the arrow keys or `j`/`k` to move, `Tab` or `h`/`l` to switch between
Formatted and Raw, and `Enter` or Space to activate a style. Mouse clicks are
supported. Press `q` or `Esc` to exit.

## UI studies

The following examples are stored in `examples/ui-studies/`:

- `button_showcase`: button states and interaction behavior
- `add_entry_gallery`: add-entry controls in table context
- `delete_icon_gallery`: delete icon and compact action variants

Each can still be started with its original example name:

```bash
cargo run --example button_showcase
cargo run --example add_entry_gallery
cargo run --example delete_icon_gallery
```
