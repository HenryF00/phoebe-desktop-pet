import React from "react";
import type { AssistantReply, AssistantState } from "@phoebe/shared";

export type InteractionMode = "assistant" | "chat";
export type PetPreviewPose = "idle" | "blink" | "wave" | "auto";
export type PetVoiceState = "preparing" | "synthesizing" | "speaking" | "idle" | "stopped" | "failed";

type PetFrame = { src: string; durationMs: number };
type PetClip = { loop: boolean; motion: string; holdLastMs?: number; frames: PetFrame[] };
type PetManifest = {
  version: number;
  canvas: { width: number; height: number };
  clips: Record<string, PetClip>;
};

const fallbackManifest: PetManifest = {
  version: 1,
  canvas: { width: 1004, height: 1567 },
  clips: {
    "assistant-idle": { loop: true, motion: "breathe", frames: [{ src: "/pet/phoebe-assistant-idle-v1.png", durationMs: 4000 }] },
    "chat-idle": { loop: true, motion: "breathe", frames: [{ src: "/pet/phoebe-chat-idle-v1.png", durationMs: 4000 }] },
  },
};

let manifestPromise: Promise<PetManifest> | null = null;

function isManifest(value: unknown): value is PetManifest {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<PetManifest>;
  return candidate.version === 1 && Boolean(candidate.clips) && typeof candidate.clips === "object";
}

function loadManifest(): Promise<PetManifest> {
  if (!manifestPromise) {
    manifestPromise = fetch("/pet/animations.json")
      .then(async response => {
        if (!response.ok) throw new Error("pet manifest unavailable");
        const value: unknown = await response.json();
        if (!isManifest(value)) throw new Error("invalid pet manifest");
        return value;
      })
      .catch(() => fallbackManifest);
  }
  return manifestPromise;
}

export function selectPetClip(input: {
  mode: InteractionMode;
  previewPose: PetPreviewPose;
  manualWave: boolean;
  assistantState: AssistantState | string;
  voiceState: PetVoiceState;
  reaction: Pick<AssistantReply, "emotion" | "gesture">;
  reactionActive: boolean;
  reactionOneShotDone: boolean;
  cancelActive: boolean;
}): string {
  const {
    mode, previewPose, manualWave, assistantState, voiceState, reaction,
    reactionActive, reactionOneShotDone, cancelActive,
  } = input;
  if (previewPose === "blink") return `${mode}-blink`;
  if (previewPose === "wave") return `${mode}-wave`;
  if (assistantState === "failed" || voiceState === "failed") return `${mode}-concerned`;
  if (cancelActive) return `${mode}-cancel`;
  if (assistantState === "tool_running") return `${mode}-tool`;
  if (voiceState === "synthesizing" || voiceState === "preparing" || assistantState === "thinking") return `${mode}-thinking`;
  if (assistantState === "listening" || assistantState === "transcribing" || assistantState === "awaiting_approval") {
    return `${mode}-listening`;
  }

  if (voiceState === "speaking" || reactionActive) {
    if (reaction.emotion === "concerned") return `${mode}-concerned`;
    if (reaction.emotion === "shy") return `${mode}-shy`;
    if (reaction.gesture === "point") return `${mode}-present`;
    if (!reactionOneShotDone && reaction.gesture === "nod") return `${mode}-nod`;
    if (!reactionOneShotDone && reaction.emotion === "happy") return `${mode}-happy`;
    return `${mode}-speaking`;
  }
  if (manualWave) return `${mode}-wave`;
  return `${mode}-idle`;
}

export function useReducedMotion(): boolean {
  const [reduced, setReduced] = React.useState(() => window.matchMedia("(prefers-reduced-motion: reduce)").matches);
  React.useEffect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReduced(query.matches);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return reduced;
}

export function usePetAnimation(clipId: string, reducedMotion: boolean, onComplete?: (clipId: string) => void) {
  const [manifest, setManifest] = React.useState<PetManifest>(fallbackManifest);
  const [frameIndex, setFrameIndex] = React.useState(0);
  const completed = React.useRef(false);
  const onCompleteRef = React.useRef(onComplete);

  React.useEffect(() => { void loadManifest().then(setManifest); }, []);
  React.useEffect(() => { onCompleteRef.current = onComplete; }, [onComplete]);
  React.useEffect(() => { setFrameIndex(0); completed.current = false; }, [clipId, reducedMotion]);

  const fallbackClip = manifest.clips[clipId.startsWith("chat-") ? "chat-idle" : "assistant-idle"]
    ?? fallbackManifest.clips[clipId.startsWith("chat-") ? "chat-idle" : "assistant-idle"];
  const clip = manifest.clips[clipId] ?? fallbackClip;
  const safeIndex = Math.min(frameIndex, clip.frames.length - 1);
  const frame = clip.frames[safeIndex] ?? fallbackClip.frames[0];

  React.useEffect(() => {
    const sources = new Set(Object.values(manifest.clips).flatMap(value => value.frames.map(item => item.src)));
    for (const src of sources) {
      const image = new Image();
      image.decoding = "async";
      image.src = src;
    }
  }, [manifest]);

  React.useEffect(() => {
    if (clip.frames.length === 0) return;
    const lastFrame = safeIndex >= clip.frames.length - 1;
    if (reducedMotion && clip.loop) return;
    const delay = Math.max(80, frame.durationMs) + (lastFrame && !clip.loop ? (clip.holdLastMs ?? 0) : 0);
    const timeout = window.setTimeout(() => {
      if (lastFrame && !clip.loop) {
        if (!completed.current) {
          completed.current = true;
          onCompleteRef.current?.(clipId);
        }
        return;
      }
      setFrameIndex(current => current + 1 < clip.frames.length ? current + 1 : 0);
    }, delay);
    return () => window.clearTimeout(timeout);
  }, [clip, clipId, frame.durationMs, reducedMotion, safeIndex]);

  return { src: frame.src, motion: reducedMotion ? "still" : clip.motion, clipId, frameIndex: safeIndex };
}
