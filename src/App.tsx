import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import {
    isPermissionGranted,
    requestPermission,
    sendNotification,
} from "@tauri-apps/plugin-notification";
import "./App.css";

type Status = "idle" | "recording" | "processing";

export default function App() {
    const [status, setStatus] = useState<Status>("idle");
    const [notificationPermissionGranted, setNotificationPermissionGranted] =
        useState(false);

    const handleRequestNotificationPermission = async () => {
        const isGranted = await isPermissionGranted();
        if (!isGranted) {
            const permission = await requestPermission();
            setNotificationPermissionGranted(permission === "granted");
        }
    };

    useEffect(() => {
        handleRequestNotificationPermission();
    }, [notificationPermissionGranted]);

    useEffect(() => {
        const unlisten = listen<string>("recording-status", (event) => {
            if (notificationPermissionGranted) {
                switch (event.payload) {
                    case "idle":
                        sendNotification({
                            title: "Prêt",
                            body: "Ctrl+Shift+Space pour enregistrer",
                        });
                        break;
                    case "recording":
                        sendNotification({
                            title: "Enregistrement en cours",
                            body: "Ctrl+Shift+Space pour stoper",
                        });
                        break;
                    case "processing":
                        sendNotification({
                            title: "Transcription en cours",
                            body: "Votre transcription est en cours",
                        });
                        break;
                }
            }
            setStatus(event.payload as Status);
        });

        return () => {
            unlisten.then((fn) => fn());
        };
    }, []);

    const statusConfig = {
        idle: {
            color: "#3b82f6",
            label: "Prêt",
            description: "Ctrl+Shift+Space pour enregistrer",
        },
        recording: {
            color: "#ef4444",
            label: "Enregistrement...",
            description: "Ctrl+Shift+Space pour stoper",
        },
        processing: {
            color: "#eab308",
            label: "Transcription...",
            description: "Veuillez patienter",
        },
    };

    const current = statusConfig[status];

    return (
        <div className="container">
            <div
                className="status-indicator"
                style={{ backgroundColor: current.color }}
            />
            <h1>{current.label}</h1>
            <p>{current.description}</p>
        </div>
    );
}
