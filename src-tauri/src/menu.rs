use tauri::menu::{MenuBuilder, PredefinedMenuItem, SubmenuBuilder};

pub fn create(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let menu = MenuBuilder::new(app)
        .item(
            &SubmenuBuilder::new(app, "File")
                .close_window()
                .build()?,
        )
        .item(
            &SubmenuBuilder::new(app, "Edit")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?,
        )
        .item(
            &SubmenuBuilder::new(app, "View")
                .item(&PredefinedMenuItem::fullscreen(app, None)?)
                .build()?,
        )
        .build()?;

    app.set_menu(menu)?;
    Ok(())
}
