#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

fn main() {
    // Charger .env
    dotenv::dotenv().ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // === SYSTEM TRAY ===
            let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Afficher", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            let _tray = TrayIconBuilder::new()
                .icon(tauri::image::Image::from_path("icons/tray.png").expect("Icone tray introuvable"))
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            // === SHORTCUT ===
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);

            // Vérifier si déjà enregistré
            if app.global_shortcut().is_registered(shortcut) {
                println!("⚠️ Raccourci déjà enregistré, on skip");
                return Ok(());
            }

            // Channel pour envoyer les commandes au thred audio
            let (tx, rx) = mpsc::channel::<&'static str>();

            // Clone app handle pour l'envoyer au thread
            let app_handle = app.handle().clone();

            // === AUDIO THREAD ===
            thread::spawn(move || {
                let mut is_recording: bool = false;
                let mut is_processing: bool = false;
                let audio_data: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::<f32>::new()));
                let mut current_stream: Option<cpal::Stream> = None;
                let mut sample_rate: u32 = 44100;
                let mut channels: u16 = 1;
                let mut last_toggle = Instant::now();

                // Creer un runtime tokio pour les appels async
                let rt = tokio::runtime::Runtime::new().unwrap();

                // Etat initial
                emit_status(&app_handle, "idle");

                loop {
                    // Attendre une commande
                    if let Ok(cmd) = rx.recv() {
                        match cmd {
                            "toggle" => {
                                // Debounce : ignorer si moins de 300ms depuis le dernier toggle
                                if last_toggle.elapsed() < Duration::from_millis(300) {
                                    continue;
                                }

                                // Ignorer si on est en train de transcrire
                                if is_processing {
                                    println!("⚠️ Transcription en cours, toggle ignoré");
                                    continue;
                                }

                                last_toggle = Instant::now();

                                if !is_recording {
                                    // Demarrer l'enregistrement
                                    println!("🎤 Enregistrement démarré...");
                                    is_recording = true;

                                    // Emit status "recording"
                                    emit_status(&app_handle, "recording");

                                    // Clear les donnees precedentes
                                    audio_data.lock().unwrap().clear();

                                    // Setup audio
                                    let host = cpal::default_host();
                                    let device =
                                        host.default_input_device().expect("Aucun micro detecte");
                                    let config =
                                        device.default_input_config().expect("Aucune config audio");

                                    sample_rate = config.sample_rate().0;
                                    channels = config.channels();

                                    println!("📍 Micro {}", device.name().unwrap_or_default());

                                    let audio_data_clone = Arc::clone(&audio_data);

                                    let stream = device
                                        .build_input_stream(
                                            &config.into(),
                                            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                                                audio_data_clone
                                                    .lock()
                                                    .unwrap()
                                                    .extend_from_slice(data);
                                            },
                                            |err| {
                                                eprintln!(
                                                    "❌ Erreur lors de la lecture audio : {}",
                                                    err
                                                )
                                            },
                                            None,
                                        )
                                        .expect("Impossible de creer le stream");

                                    stream.play().expect("Impossible de démarrer le stream");
                                    current_stream = Some(stream);
                                } else {
                                    // Arrêter l'enregistrement
                                    println!("🛑 Enregistrement arrêté");
                                    is_recording = false;
                                    is_processing = true; // Bloquer les toggles

                                    // Emit status "processing"
                                    emit_status(&app_handle, "processing");

                                    // Drop le stream pour arreter la capture
                                    current_stream = None;

                                    // Sauvegarder en WAV
                                    let data = audio_data.lock().unwrap();
                                    if !data.is_empty() {
                                        let spec = WavSpec {
                                            channels,
                                            sample_rate,
                                            bits_per_sample: 32,
                                            sample_format: hound::SampleFormat::Float,
                                        };

                                        let path = "../recording.wav";
                                        let mut writer = WavWriter::create(path, spec)
                                            .expect("Impossible de creer le WAV");

                                        for sample in data.iter() {
                                            writer.write_sample(*sample).unwrap();
                                        }

                                        writer.finalize().unwrap();
                                        println!(
                                            "💾 Enregistrement sauvegardé: {} ({} samples)",
                                            path,
                                            data.len()
                                        );

                                        // Transcrire avec Groq
                                        println!("🔄 Transcription en cours...");
                                        match rt.block_on(transcribe_audio(path)) {
                                            Ok(text) => {
                                                println!("✅ Transcription réussie: {}", text);

                                                // Copier dans le presse-papier
                                                match arboard::Clipboard::new() {
                                                    Ok(mut clipboard) => {
                                                        if clipboard.set_text(&text).is_ok() {
                                                            println!("📋 Copié !");

                                                            // Petit delai pour laisser le presse-paier se mettre a jour
                                                            std::thread::sleep(Duration::from_millis(100));

                                                            // Simuler Ctrl+V pour coller
                                                            if let Ok(mut enigo) = enigo::Enigo::new(&enigo::Settings::default()) {
                                                                use enigo::{Direction, Key, Keyboard};
                                                                let _ = enigo.key(Key::Control, Direction::Press);
                                                                let _ = enigo.key(Key::Unicode('v'), Direction::Click);
                                                                let _ = enigo.key(Key::Control, Direction::Release);
                                                                println!("⌨️ Collé automatiquement !")
                                                            }
                                                        }
                                                    }
                                                    Err(err) => eprintln!("❌ Erreur lors de la copie dans le presse-papier: {}", err),
                                                }
                                            }
                                            Err(err) => eprintln!("❌ Clipboard error: {}", err),
                                        }

                                        is_processing = false; // Débloquer les toggles

                                        // Vider les messages en attentes (fix hot-reload)
                                        while rx.try_recv().is_ok() {}

                                        // Emit status "idle"
                                        emit_status(&app_handle, "idle");
                                        println!("✅ Traitement terminé. Prêt pour le prochain enregistrement");
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            });

            // Enregistrer et Écouter le raccourci
            match app
                .global_shortcut()
                .on_shortcut(shortcut, move |_app, _shortcut, _event| {
                    let _ = tx.send("toggle");
                }) {
                Ok(_) => println!("✅ Listner attache"),
                Err(e) => println!("❌ Erreur lors de l'attache du listener {:?}", e),
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Cacher la fenetre au lieu de fermer quand on clique sur la croix "X"
            if let tauri::WindowEvent::CloseRequested { api, ..} = event {
                let _ = window.hide().unwrap();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn emit_status(app: &AppHandle, status: &str) {
    let _ = app.emit("recording-status", status);
}

async fn transcribe_audio(file_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let api_key = std::env::var("GROQ_API_KEY").expect("GROQ_API_KEY non definie");

    let file_bytes = std::fs::read(file_path)?;
    let file_part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("model", "whisper-large-v3-turbo");

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.groq.com/openai/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(format!("Erreur lors de la transcription: {}", error_text).into());
    }

    let json: serde_json::Value = response.json().await?;
    let text = json["text"].as_str().unwrap_or("").to_string();

    Ok(text)
}
