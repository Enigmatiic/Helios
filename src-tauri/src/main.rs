#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);

            // Vérifier si déjà enregistré
            if app.global_shortcut().is_registered(shortcut) {
                println!("⚠️ Raccourci déjà enregistré, on skip");
                return Ok(());
            }

            // Channel pour envoyer les commandes au thred audio
            let (tx, rx) = mpsc::channel::<&'static str>();

            // Thread dedie a l'audio
            thread::spawn(move || {
                let mut is_recording: bool = false;
                let audio_data: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::<f32>::new()));
                let mut current_stream: Option<cpal::Stream> = None;
                let mut sample_rate: u32 = 44100;
                let mut channels: u16 = 1;
                let mut last_toggle = Instant::now();

                loop {
                    // Attendre une commande
                    if let Ok(cmd) = rx.recv() {
                        match cmd {
                            "toggle" => {
                                // Debounce : ignorer si moins de 300ms depuis le dernier toggle
                                if last_toggle.elapsed() < Duration::from_millis(300) {
                                    continue;
                                }
                                last_toggle = Instant::now();

                                if !is_recording {
                                    // Demarrer l'enregistrement
                                    println!("🎤 Enregistrement démarré...");
                                    is_recording = true;

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
                                    println!("📍 Config audio : {:?}", config);

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

                                        let path = "recording.wav";
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
