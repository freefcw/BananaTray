use gpui::App;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

const GPU_CACHE_TRIM_DELAY: Duration = Duration::from_millis(100);

pub fn register_idle_gpu_cache_trim(cx: &mut App) {
    let trim_pending = Rc::new(Cell::new(false));
    cx.on_window_closed(move |cx| {
        if trim_pending.replace(true) {
            return;
        }

        let trim_pending = trim_pending.clone();
        let async_cx = cx.to_async();
        let trim_cx = async_cx.clone();
        async_cx
            .foreground_executor()
            .spawn(async move {
                gpui::Timer::after(GPU_CACHE_TRIM_DELAY).await;
                let _ = trim_cx.update(move |cx| {
                    trim_pending.set(false);
                    if cx.windows().is_empty() {
                        cx.trim_gpu_caches();
                    }
                });
            })
            .detach();
    })
    .detach();
}
