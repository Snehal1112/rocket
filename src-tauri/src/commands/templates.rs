use rocket_app::TemplateService;
use rocket_history::Template;
use rocket_shared::error::DomainError;
use tauri::State;

#[tauri::command]
pub fn list_templates(svc: State<'_, TemplateService>) -> Result<Vec<Template>, DomainError> {
    svc.list()
}

#[tauri::command]
pub fn get_template(
    name: String,
    svc: State<'_, TemplateService>,
) -> Result<Template, DomainError> {
    svc.get(&name)
}

#[tauri::command]
pub fn save_template(
    template: Template,
    svc: State<'_, TemplateService>,
) -> Result<(), DomainError> {
    svc.save(&template)
}

#[tauri::command]
pub fn delete_template(
    name: String,
    svc: State<'_, TemplateService>,
) -> Result<(), DomainError> {
    svc.delete(&name)
}
