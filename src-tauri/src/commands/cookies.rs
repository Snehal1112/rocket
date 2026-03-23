use rocket_app::CookieService;
use rocket_http::CookieJar;
use rocket_shared::error::DomainError;
use tauri::State;

#[tauri::command]
pub fn get_cookies(svc: State<'_, CookieService>) -> Result<Vec<CookieJar>, DomainError> {
    svc.get_all()
}

#[tauri::command]
pub fn set_cookies(jar: CookieJar, svc: State<'_, CookieService>) -> Result<(), DomainError> {
    svc.save(&jar)
}

#[tauri::command]
pub fn clear_cookies(svc: State<'_, CookieService>) -> Result<(), DomainError> {
    svc.clear()
}
