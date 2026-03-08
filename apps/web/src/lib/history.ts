import { createSignal } from "solid-js";

export interface Command {
  execute: () => Promise<void> | void;
  undo: () => Promise<void> | void;
  description: string;
}

export interface HistoryStore {
  execute: (command: Command) => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  canUndo: () => boolean;
  canRedo: () => boolean;
  clear: () => void;
}

const MAX_HISTORY = 50;

export function createHistoryStore(): HistoryStore {
  const [undoStack, setUndoStack] = createSignal<Command[]>([]);
  const [redoStack, setRedoStack] = createSignal<Command[]>([]);

  const execute = async (command: Command) => {
    await command.execute();
    setUndoStack((prev) => {
      const next = [...prev, command];
      if (next.length > MAX_HISTORY) next.shift();
      return next;
    });
    setRedoStack([]);
  };

  const undo = async () => {
    const stack = undoStack();
    if (stack.length === 0) return;

    const command = stack[stack.length - 1];
    await command.undo();

    setUndoStack((prev) => prev.slice(0, -1));
    setRedoStack((prev) => [...prev, command]);
  };

  const redo = async () => {
    const stack = redoStack();
    if (stack.length === 0) return;

    const command = stack[stack.length - 1];
    await command.execute();

    setRedoStack((prev) => prev.slice(0, -1));
    setUndoStack((prev) => [...prev, command]);
  };

  return {
    execute,
    undo,
    redo,
    canUndo: () => undoStack().length > 0,
    canRedo: () => redoStack().length > 0,
    clear: () => {
      setUndoStack([]);
      setRedoStack([]);
    },
  };
}
