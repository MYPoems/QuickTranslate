use crate::{
    app::AppState,
    errors::AppError,
    update::{self, UpdateInfo, UpdatePhase, UpdateProgress},
};
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct UpdateSession {
    progress: UpdateProgress,
    update: Option<Update>,
    bytes: Vec<u8>,
    cancel: CancellationToken,
}
#[derive(Default)]
pub struct UpdateManager(Mutex<UpdateSession>);

fn update_error(error: impl std::fmt::Display) -> AppError {
    AppError::Update(format!("更新失败：{error}"))
}
fn publish(app: &AppHandle, progress: &UpdateProgress) {
    let _ = app.emit("update-progress", progress);
}
fn busy(phase: &UpdatePhase) -> bool {
    matches!(
        phase,
        UpdatePhase::Checking
            | UpdatePhase::Downloading
            | UpdatePhase::Verifying
            | UpdatePhase::Installing
    )
}

#[tauri::command]
pub fn get_update_state(app: AppHandle) -> Result<UpdateProgress, AppError> {
    Ok(app
        .state::<UpdateManager>()
        .0
        .lock()
        .map_err(update_error)?
        .progress
        .clone())
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateInfo, AppError> {
    {
        let manager = app.state::<UpdateManager>();
        let mut session = manager.0.lock().map_err(update_error)?;
        if busy(&session.progress.phase) {
            return Err(update_error("更新任务正在进行"));
        }
        *session = UpdateSession::default();
        session.progress.phase = UpdatePhase::Checking;
        publish(&app, &session.progress);
    }
    let checked = async {
        let info = update::check_for_updates(&app.state::<AppState>().http_client).await?;
        // Older releases do not have a signed feed. No need to fetch it when already current.
        let candidate = if info.update_available {
            app.updater_builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(update_error)?
                .check()
                .await
                .map_err(update_error)?
        } else {
            None
        };
        if info.update_available && candidate.is_none() {
            return Err(update_error("该 Release 未提供可验证的更新包，请稍后重试"));
        }
        if let Some(update) = &candidate {
            if update.version != info.latest_version {
                return Err(update_error("Release 与更新清单版本不一致"));
            }
            update::validate_download_url(&update.download_url, &update.version)?;
        }
        Ok::<_, AppError>((info, candidate))
    }
    .await;
    let manager = app.state::<UpdateManager>();
    let mut session = manager.0.lock().map_err(update_error)?;
    match checked {
        Ok((info, candidate)) => {
            session.progress = UpdateProgress {
                phase: if candidate.is_some() {
                    UpdatePhase::Available
                } else {
                    UpdatePhase::Idle
                },
                version: info.latest_version.clone(),
                release_notes: candidate
                    .as_ref()
                    .and_then(|update| update.body.clone())
                    .unwrap_or_default(),
                message: if candidate.is_some() {
                    "发现新版本，可下载并校验".into()
                } else {
                    format!("当前 v{} 已是最新正式版", info.current_version)
                },
                ..Default::default()
            };
            session.update = candidate;
            publish(&app, &session.progress);
            Ok(info)
        }
        Err(error) => {
            session.progress.phase = UpdatePhase::Error;
            session.progress.message = error.user_message();
            publish(&app, &session.progress);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn download_update(app: AppHandle) -> Result<(), AppError> {
    let (mut candidate, cancel) = {
        let manager = app.state::<UpdateManager>();
        let mut session = manager.0.lock().map_err(update_error)?;
        if !matches!(
            session.progress.phase,
            UpdatePhase::Available | UpdatePhase::Cancelled | UpdatePhase::Error
        ) {
            return Err(update_error("请先检查更新，或等待现有任务完成"));
        }
        let candidate = session
            .update
            .clone()
            .ok_or_else(|| update_error("请先检查更新"))?;
        session.bytes.clear();
        session.cancel = CancellationToken::new();
        session.progress.phase = UpdatePhase::Downloading;
        session.progress.downloaded = 0;
        session.progress.total = None;
        session.progress.message.clear();
        publish(&app, &session.progress);
        (candidate, session.cancel.clone())
    };
    candidate.timeout = Some(Duration::from_secs(600));
    let result = cancel
        .run_until_cancelled(candidate.download(
            |chunk, total| {
                if let Ok(mut session) = app.state::<UpdateManager>().0.lock() {
                    session.progress.downloaded += chunk as u64;
                    session.progress.total = total;
                    if session.progress.downloaded > 150 * 1024 * 1024
                        || total.is_some_and(|size| size > 150 * 1024 * 1024)
                    {
                        cancel.cancel();
                    }
                    publish(&app, &session.progress);
                }
            },
            || {
                if let Ok(mut session) = app.state::<UpdateManager>().0.lock() {
                    session.progress.phase = UpdatePhase::Verifying;
                    publish(&app, &session.progress);
                }
            },
        ))
        .await;
    let manager = app.state::<UpdateManager>();
    let mut session = manager.0.lock().map_err(update_error)?;
    let returned = match result {
        Some(Ok(bytes)) if !cancel.is_cancelled() => {
            session.bytes = bytes;
            session.progress.phase = UpdatePhase::Ready;
            session.progress.message =
                "签名与版本校验通过。安装前会自动备份设置与历史，必须再次确认。".into();
            Ok(())
        }
        Some(Err(error)) => {
            session.bytes.clear();
            session.progress.phase = UpdatePhase::Error;
            session.progress.message = format!("下载或签名校验失败，未执行安装：{error}");
            Err(update_error(error))
        }
        _ => {
            session.bytes.clear();
            session.progress.phase = UpdatePhase::Cancelled;
            session.progress.message = "下载已取消，当前版本未改变".into();
            Ok(())
        }
    };
    publish(&app, &session.progress);
    returned
}

#[tauri::command]
pub fn cancel_update_download(app: AppHandle) -> Result<(), AppError> {
    let manager = app.state::<UpdateManager>();
    let session = manager.0.lock().map_err(update_error)?;
    if matches!(
        session.progress.phase,
        UpdatePhase::Downloading | UpdatePhase::Verifying
    ) {
        session.cancel.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    version: String,
    confirmed: bool,
) -> Result<(), AppError> {
    let (candidate, bytes) = {
        let manager = app.state::<UpdateManager>();
        let mut session = manager.0.lock().map_err(update_error)?;
        update::validate_install(
            &session.progress.phase,
            &session.progress.version,
            &version,
            confirmed,
            &session.bytes,
        )?;
        let candidate = session
            .update
            .clone()
            .ok_or_else(|| update_error("更新状态已失效"))?;
        session.progress.phase = UpdatePhase::Installing;
        session.progress.message = "正在备份设置与历史，然后启动安装程序…".into();
        publish(&app, &session.progress);
        (candidate, std::mem::take(&mut session.bytes))
    };
    let result = async {
        let state = app.state::<AppState>();
        let config_dir = state
            .settings
            .path()
            .parent()
            .ok_or_else(|| update_error("设置路径无效"))?
            .to_path_buf();
        let backup_root = state
            .cache_path
            .parent()
            .ok_or_else(|| update_error("历史路径无效"))?
            .join("upgrade-backups");
        let settings = state.settings.get()?;
        let size = state.popup_size.current();
        let snapshot = tokio::task::spawn_blocking(move || {
            crate::upgrade::begin_backup(&backup_root, &config_dir, &settings, size, &version)
        })
        .await
        .map_err(update_error)??;
        state
            .translation
            .backup_cache(snapshot.join("translations.sqlite3"))
            .await?;
        tokio::task::spawn_blocking(move || crate::upgrade::finish_backup(&snapshot))
            .await
            .map_err(update_error)??;
        tokio::task::spawn_blocking(move || candidate.install(&bytes).map_err(update_error))
            .await
            .map_err(update_error)?
    }
    .await;
    if let Err(error) = &result {
        let manager = app.state::<UpdateManager>();
        let mut session = manager.0.lock().map_err(update_error)?;
        session.progress.phase = UpdatePhase::Error;
        session.progress.message = format!(
            "安装未完成：{}。当前数据保留，请重新下载后重试。",
            error.user_message()
        );
        publish(&app, &session.progress);
    }
    result
}
