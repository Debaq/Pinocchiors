import { Component, For, Show, createEffect, createMemo, createSignal } from "solid-js";
import { Button } from "../ui";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";
import { chat, collabPanelTab, sendChat, setChatUnread, setCollabPanelTab, status } from "../../lib/collab";
import {
  addReply,
  addThread,
  deleteThread,
  draft,
  goToThread,
  placing,
  selectedThread,
  setDraft,
  setPlacing,
  setResolved,
  setSelectedThread,
  threads,
  type Thread,
} from "../../lib/collabComments";
import { ConfirmDialog } from "./ConfirmDialog";

const inputClass = "w-full px-2 py-1 rounded border border-border bg-surface/40 text-sm text-text outline-none focus:border-accent";

const when = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

/**
 * Panel izquierdo de colaboración, con pestañas: Chat de la sala y Comentarios
 * anclados a la pieza. Los comentarios sirven también sin sala (anotaciones del
 * proyecto); con sala se comparten.
 */
export const CollabPanel: Component<{ onClose: () => void; onAskAi?: (text: string) => void }> = (props) => {
  const tab = collabPanelTab;

  // Mirar el chat lo deja leído
  createEffect(() => {
    if (tab() === "chat") setChatUnread(0);
  });

  return (
    <aside class="shrink-0 w-80 min-w-0 flex flex-col border-r border-border bg-bg" aria-label={t("Colaboración")}>
      <div class="flex items-center gap-1 px-2 h-9 border-b border-border">
        <button
          class="px-2 py-1 text-xs rounded"
          classList={{ "bg-accent/20 text-text": tab() === "chat", "text-text-muted hover:text-text": tab() !== "chat" }}
          onClick={() => setCollabPanelTab("chat")}
        >
          {t("Chat")}
        </button>
        <button
          class="px-2 py-1 text-xs rounded"
          classList={{ "bg-accent/20 text-text": tab() === "comments", "text-text-muted hover:text-text": tab() !== "comments" }}
          onClick={() => setCollabPanelTab("comments")}
        >
          {t("Comentarios")}
          <Show when={threads().filter((x) => !x.resolved).length > 0}>
            <span class="ml-1 text-text-muted">{threads().filter((x) => !x.resolved).length}</span>
          </Show>
        </button>
        <span class="flex-1" />
        <button class="p-1 text-text-muted hover:text-text" aria-label={t("Cerrar")} onClick={props.onClose}>
          <Icons.X size={14} />
        </button>
      </div>
      <Show when={tab() === "chat"} fallback={<CommentsTab onAskAi={props.onAskAi} />}>
        <ChatTab />
      </Show>
    </aside>
  );
};

const ChatTab: Component = () => {
  const [line, setLine] = createSignal("");
  let list: HTMLDivElement | undefined;
  createEffect(() => {
    chat();
    queueMicrotask(() => list?.scrollTo({ top: list.scrollHeight }));
  });
  const send = () => {
    const text = line();
    setLine("");
    void sendChat(text).catch(() => {});
  };
  return (
    <Show
      when={status().on}
      fallback={<p class="p-3 text-xs text-text-muted leading-relaxed">{t("El chat funciona dentro de una sala. Crea o únete a una en Configuración ▸ Colaborar.")}</p>}
    >
      <div ref={list} class="flex-1 min-h-0 overflow-y-auto p-3 space-y-2 text-sm">
        <For each={chat()} fallback={<p class="text-xs text-text-muted">{t("Nadie ha escrito todavía.")}</p>}>
          {(c) => (
            <Show
              when={!c.system}
              fallback={<div class="text-center text-[11px] text-text-muted">{c.text}</div>}
            >
            <div classList={{ "text-right": c.mine }}>
              <Show when={!c.mine}>
                <div class="text-[10px] text-text-muted">{c.name}</div>
              </Show>
              <span class="inline-block max-w-full px-2 py-1 rounded-lg break-words text-left" classList={{ "bg-accent/25": c.mine, "bg-surface": !c.mine }}>
                {c.text}
              </span>
            </div>
            </Show>
          )}
        </For>
      </div>
      <div class="p-2 border-t border-border">
        <input
          class={inputClass}
          value={line()}
          placeholder={t("Escribe y presiona Enter")}
          onInput={(e) => setLine(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") send();
          }}
        />
      </div>
    </Show>
  );
};

const CommentsTab: Component<{ onAskAi?: (text: string) => void }> = (props) => {
  const [showResolved, setShowResolved] = createSignal(false);
  const [text, setText] = createSignal("");
  const [confirmDelete, setConfirmDelete] = createSignal<string>();
  const visible = createMemo(() => threads().filter((x) => showResolved() || !x.resolved || x.id === selectedThread()));

  const submitDraft = () => {
    const a = draft();
    if (!a) return;
    if (addThread(a, text())) {
      setDraft(undefined);
      setText("");
    }
  };

  return (
    <div class="flex-1 min-h-0 flex flex-col">
      <div class="p-2 space-y-2 border-b border-border">
        <Button
          variant={placing() ? "primary" : undefined}
          onClick={() => {
            setDraft(undefined);
            setPlacing(!placing());
          }}
        >
          {placing() ? t("Haz clic en el visor… (Esc cancela)") : t("Poner un comentario")}
        </Button>
        <label class="flex items-center gap-2 text-xs text-text-muted">
          <input type="checkbox" checked={showResolved()} onChange={(e) => setShowResolved(e.currentTarget.checked)} />
          {t("Mostrar resueltos")}
        </label>
      </div>

      <Show when={draft()}>
        <div class="p-2 space-y-2 border-b border-border">
          <div class="text-xs text-text-muted">{draft()!.target ? t("Nuevo comentario en {t}", { t: draft()!.target! }) : t("Nuevo comentario")}</div>
          <textarea
            class={`${inputClass} h-16 resize-none`}
            value={text()}
            placeholder={t("Escribe el comentario")}
            autofocus
            onInput={(e) => setText(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submitDraft();
              } else if (e.key === "Escape") setDraft(undefined);
            }}
          />
          <div class="flex gap-2">
            <Button variant="primary" disabled={!text().trim()} onClick={submitDraft}>
              {t("Publicar")}
            </Button>
            <Button onClick={() => setDraft(undefined)}>{t("Cancelar")}</Button>
          </div>
        </div>
      </Show>

      <div class="flex-1 min-h-0 overflow-y-auto">
        <For
          each={visible()}
          fallback={<p class="p-3 text-xs text-text-muted leading-relaxed">{t("Sin comentarios. «Poner un comentario» y haz clic en la pieza donde quieras dejarlo.")}</p>}
        >
          {(th) => <ThreadCard thread={th} onAskAi={props.onAskAi} onDelete={() => setConfirmDelete(th.id)} />}
        </For>
      </div>

      <Show when={confirmDelete()}>
        <ConfirmDialog
          title={t("Borrar el comentario")}
          message={t("Se borra el hilo completo, para todos en la sala.")}
          confirmLabel={t("Borrar")}
          danger
          onConfirm={() => {
            deleteThread(confirmDelete()!);
            setConfirmDelete(undefined);
          }}
          onCancel={() => setConfirmDelete(undefined)}
        />
      </Show>
    </div>
  );
};

const ThreadCard: Component<{ thread: Thread; onDelete: () => void; onAskAi?: (text: string) => void }> = (props) => {
  const open = () => selectedThread() === props.thread.id;
  const [reply, setReply] = createSignal("");
  const first = () => props.thread.comments[0];
  const send = () => {
    addReply(props.thread.id, reply());
    setReply("");
  };
  const asAi = () => {
    const th = props.thread;
    const where = th.anchor.target ? ` ${t("sobre {t}", { t: th.anchor.target })}` : "";
    const body = th.comments.map((c) => `${c.author}: ${c.text}`).join("\n");
    props.onAskAi?.(t("Comentario #{n}{where}:", { n: th.n, where }) + "\n" + body);
  };
  return (
    <div class="border-b border-border" classList={{ "bg-accent/10": open(), "opacity-60": props.thread.resolved }}>
      <button
        class="w-full text-left px-3 py-2 flex gap-2 items-start"
        onClick={() => {
          setSelectedThread(open() ? undefined : props.thread.id);
          if (!open()) goToThread(props.thread.id);
        }}
      >
        <span class="shrink-0 w-5 h-5 rounded-full text-[10px] flex items-center justify-center text-white" style={{ background: first()?.color }}>
          {props.thread.n}
        </span>
        <span class="min-w-0 flex-1">
          <span class="block text-xs text-text-muted truncate">
            {first()?.author}
            <Show when={props.thread.anchor.target}>{` · ${props.thread.anchor.target}`}</Show>
            {props.thread.resolved ? ` · ${t("resuelto")}` : ""}
          </span>
          <span class="block text-sm text-text" classList={{ truncate: !open() }}>
            {first()?.text}
          </span>
        </span>
        <Show when={props.thread.comments.length > 1 && !open()}>
          <span class="text-xs text-text-muted">{props.thread.comments.length - 1}</span>
        </Show>
      </button>
      <Show when={open()}>
        <div class="px-3 pb-3 space-y-2">
          <For each={props.thread.comments.slice(1)}>
            {(c) => (
              <div class="text-sm">
                <span class="text-xs" style={{ color: c.color }}>
                  {c.author}
                </span>
                <span class="text-[10px] text-text-muted"> {when(c.at)}</span>
                <div class="text-text break-words">{c.text}</div>
              </div>
            )}
          </For>
          <input
            class={inputClass}
            value={reply()}
            placeholder={t("Responder…")}
            onInput={(e) => setReply(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") send();
            }}
          />
          <div class="flex flex-wrap gap-2">
            <Button onClick={() => setResolved(props.thread.id, !props.thread.resolved)}>{props.thread.resolved ? t("Reabrir") : t("Resolver")}</Button>
            <Show when={props.thread.anchor.p}>
              <Button onClick={() => goToThread(props.thread.id)}>{t("Ir al punto")}</Button>
            </Show>
            <Show when={props.onAskAi}>
              <Button onClick={asAi}>{t("Preguntar a la IA")}</Button>
            </Show>
            <Button onClick={props.onDelete}>{t("Borrar")}</Button>
          </div>
        </div>
      </Show>
    </div>
  );
};
