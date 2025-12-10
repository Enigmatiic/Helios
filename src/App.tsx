import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type Status = "idle" | "recording" | "processing";

export default function App() {
    const [status, setStatus] = useState<Status>("idle");

    useEffect(() => {
        const unlisten = listen<string>("recording-status", (event) => {
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
            description: "Ctrl+Shift+Space pour enregistrer",
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
