use android_activity::AndroidApp;
use winit::platform::android::EventLoopBuilderExtAndroid;

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info));
    let event_loop = winit::event_loop::EventLoopBuilder::new().with_android_app(app).build();
    // Minimal: real app wiring lives once riode exposes an Android runner.
    let _ = event_loop;
    log::info!("riode-android: scaffold running");
}
