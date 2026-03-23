use rocket_app::CollectionService;
use rocket_collection::{Collection, CollectionSummary, Request};
use rocket_shared::error::DomainError;
use tauri::State;

#[tauri::command]
pub fn list_collections(
    svc: State<'_, CollectionService>,
) -> Result<Vec<CollectionSummary>, DomainError> {
    svc.list()
}

#[tauri::command]
pub fn get_collection(
    name: String,
    svc: State<'_, CollectionService>,
) -> Result<Collection, DomainError> {
    svc.get(&name)
}

#[tauri::command]
pub fn create_collection(
    name: String,
    svc: State<'_, CollectionService>,
) -> Result<Collection, DomainError> {
    svc.create(&name)
}

#[tauri::command]
pub fn delete_collection(
    name: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.delete(&name)
}

#[tauri::command]
pub fn rename_collection(
    old_name: String,
    new_name: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.rename(&old_name, &new_name)
}

#[tauri::command]
pub fn save_request(
    collection: String,
    path: String,
    request: Request,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.save_request(&collection, &path, &request)
}

#[tauri::command]
pub fn delete_request(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.delete_request(&collection, &path)
}

#[tauri::command]
pub fn create_folder(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.create_folder(&collection, &path)
}

#[tauri::command]
pub fn delete_folder(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.delete_folder(&collection, &path)
}

#[tauri::command]
pub fn move_item(
    src_collection: String,
    src_path: String,
    dst_collection: String,
    dst_path: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.move_item(&src_collection, &src_path, &dst_collection, &dst_path)
}
