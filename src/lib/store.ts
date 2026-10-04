// Tiny shared stores (no state library), same pattern as DeskZero.

import { useSyncExternalStore } from "react";
import { api, errorText } from "./api";
import type { AppInfo } from "./types";

function createStore<T>(initial: T) {
  let value = initial;
  const subs = new Set<() => void>();
  return {
    get: () => value,
    set(next: T) {
      value = next;
      subs.forEach((s) => s());
    },
    subscribe(fn: () => void) {
      subs.add(fn);
      return () => subs.delete(fn);
    },
  };
}

const infoStore = createStore<AppInfo | null>(null);
const toastStore = createStore<{ id: number; text: string; error: boolean }[]>([]);

export function useAppInfo() {
  return useSyncExternalStore(infoStore.subscribe, infoStore.get);
}

export function useToasts() {
  return useSyncExternalStore(toastStore.subscribe, toastStore.get);
}

let toastId = 0;
export function toast(text: string, error = false) {
  const id = ++toastId;
  toastStore.set([...toastStore.get(), { id, text, error }]);
  setTimeout(() => toastStore.set(toastStore.get().filter((t) => t.id !== id)), error ? 7000 : 3500);
}

export function toastError(e: unknown) {
  toast(errorText(e), true);
}

export async function refreshAppInfo() {
  try {
    infoStore.set(await api.appInfo());
  } catch (e) {
    toastError(e);
  }
}
