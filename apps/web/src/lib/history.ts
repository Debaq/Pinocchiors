import { batch, createSignal } from "solid-js";

/**
 * Historial en árbol: deshacer y hacer otra cosa abre una rama nueva en vez
 * de perder lo deshecho. Cada paso es un dato (`kind` + `data`), no una
 * función, así el árbol entero se guarda en el proyecto y se puede recorrer
 * al reabrirlo. Qué hace cada tipo de paso lo dicen los `StepHandlers`.
 *
 * Las operaciones que todavía no se pueden deshacer (importar, autorig,
 * retopología…) quedan como hitos: aparecen en la línea de tiempo pero
 * deshacer no los cruza. Cuando una pase a ser deshacible, basta con darle
 * un handler.
 */

export interface Step {
  kind: string;
  data?: unknown;
}

export interface StepHandler {
  /** Aplica el paso. `first` = la primera vez (puede que ya esté aplicado) */
  apply: (data: never, first: boolean) => Promise<void> | void;
  /** Lo deshace */
  revert: (data: never) => Promise<void> | void;
}

export type StepHandlers = Record<string, StepHandler>;

export interface HistoryNode {
  id: number;
  parent: number | null;
  children: number[];
  description: string;
  /** Milisegundos desde 1970 */
  time: number;
  /** `null` en la raíz */
  step: Step | null;
  /** Hito: queda registrado pero no se puede deshacer */
  milestone?: boolean;
  /** Hijo por el que sigue Rehacer (el último visitado) */
  redoChild?: number;
}

export interface SavedHistory {
  version: 1;
  nodes: HistoryNode[];
  current: number;
}

export interface HistoryStore {
  /** Aplica un paso y lo agrega como hijo del actual. `applied`: ya estaba aplicado */
  execute: (description: string, step: Step, options?: { applied?: boolean }) => Promise<void>;
  /** Registra un hito (operación ya hecha que no se puede deshacer) */
  milestone: (description: string) => void;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  /** Va a cualquier nodo del árbol: deshace hasta el ancestro común y rehace hasta él */
  goTo: (id: number) => Promise<void>;
  canUndo: () => boolean;
  canRedo: () => boolean;
  /** ¿Se puede llegar a `id` sin cruzar un hito hacia atrás? */
  canReach: (id: number) => boolean;
  nodes: () => readonly HistoryNode[];
  current: () => number;
  /** Convierte en hitos los pasos que ya no se pueden deshacer */
  seal: (match: (step: Step) => boolean) => void;
  /** Solo la raíz (modelo nuevo) */
  clear: () => void;
  save: () => SavedHistory;
  /** `volatile`: tipos de paso que dependen de un estado que no se guardó;
   *  al cargar pasan a ser hitos */
  load: (saved: SavedHistory | undefined, volatile?: string[]) => void;
}

const root = (): HistoryNode => ({ id: 0, parent: null, children: [], description: "Inicio", time: Date.now(), step: null });

export function createHistoryStore(handlers: StepHandlers): HistoryStore {
  const [nodes, setNodes] = createSignal<HistoryNode[]>([root()], { equals: false });
  const [current, setCurrent] = createSignal(0);
  // Un paso a la vez: deshacer mientras otro se aplica cruzaría los estados
  let busy: Promise<void> = Promise.resolve();
  const serial = (task: () => Promise<void>) => (busy = busy.then(task, task));

  const handler = (step: Step) => {
    const h = handlers[step.kind];
    if (!h) throw new Error(`Paso de historial desconocido: ${step.kind}`);
    return h as unknown as { apply: (d: unknown, first: boolean) => Promise<void> | void; revert: (d: unknown) => Promise<void> | void };
  };

  const add = (node: Omit<HistoryNode, "id" | "parent" | "children">) => {
    const list = nodes();
    const parent = list[current()];
    const id = list.length;
    list.push({ ...node, id, parent: parent.id, children: [] });
    parent.children.push(id);
    parent.redoChild = id;
    setNodes(list);
    setCurrent(id);
  };

  const undoOne = async () => {
    const node = nodes()[current()];
    if (node.parent === null || node.milestone || !node.step) return false;
    await handler(node.step).revert(node.step.data);
    const parent = nodes()[node.parent];
    parent.redoChild = node.id;
    setCurrent(parent.id);
    setNodes(nodes());
    return true;
  };

  const redoTo = async (childId: number) => {
    const child = nodes()[childId];
    if (child.step && !child.milestone) await handler(child.step).apply(child.step.data, false);
    setCurrent(childId);
    setNodes(nodes());
  };

  /** Camino de la raíz a `id` */
  const path = (id: number) => {
    const out: number[] = [];
    for (let n: number | null = id; n !== null; n = nodes()[n].parent) out.push(n);
    return out.reverse();
  };

  const canReach = (id: number) => {
    const from = path(current());
    const to = new Set(path(id));
    // Los nodos que hay que deshacer (del actual hacia arriba, hasta el ancestro común)
    for (let i = from.length - 1; i >= 0 && !to.has(from[i]); i--) {
      if (nodes()[from[i]].milestone) return false;
    }
    // Ni rehacer un hito, del ancestro común hacia `id`
    const shared = new Set(from);
    return [...to].every((n) => shared.has(n) || !nodes()[n].milestone);
  };

  const redoTarget = () => {
    const node = nodes()[current()];
    const next = node.redoChild ?? node.children[node.children.length - 1];
    // Un hito no se puede rehacer (solo pasa con los pasos que dejaron de ser
    // deshacibles al cargar el proyecto)
    return next === undefined || nodes()[next].milestone ? undefined : next;
  };

  return {
    execute: (description, step, options) =>
      serial(async () => {
        if (!options?.applied) await handler(step).apply(step.data, true);
        add({ description, time: Date.now(), step });
      }),
    milestone: (description) => add({ description, time: Date.now(), step: null, milestone: true }),
    undo: () => serial(async () => void (await undoOne())),
    redo: () =>
      serial(async () => {
        const next = redoTarget();
        if (next !== undefined) await redoTo(next);
      }),
    goTo: (id) =>
      serial(async () => {
        if (!nodes()[id] || !canReach(id)) return;
        const target = path(id);
        const onPath = new Set(target);
        while (!onPath.has(current())) {
          if (!(await undoOne())) return;
        }
        for (let i = target.indexOf(current()) + 1; i < target.length; i++) {
          nodes()[target[i - 1]].redoChild = target[i];
          await redoTo(target[i]);
        }
      }),
    canUndo: () => {
      const node = nodes()[current()];
      return node.parent !== null && !node.milestone;
    },
    canRedo: () => redoTarget() !== undefined,
    canReach,
    nodes,
    current,
    seal: (match) => {
      let changed = false;
      for (const node of nodes()) {
        if (node.step && !node.milestone && match(node.step)) {
          node.milestone = true;
          changed = true;
        }
      }
      if (changed) setNodes(nodes());
    },
    // Lista e índice juntos: con la lista nueva y el índice viejo, nodes()[current()] no existe
    clear: () =>
      batch(() => {
        setNodes([root()]);
        setCurrent(0);
      }),
    save: () => ({ version: 1, nodes: nodes(), current: current() }),
    load: (saved, volatile = []) =>
      batch(() => {
        const valid =
          saved?.version === 1 &&
          Array.isArray(saved.nodes) &&
          saved.nodes.length > 0 &&
          !!saved.nodes[saved.current] &&
          saved.nodes.every((n) => n && Array.isArray(n.children));
        if (!valid) {
          setNodes([root()]);
          setCurrent(0);
          return;
        }
        setNodes(
          saved!.nodes.map((n) => ({
            ...n,
            children: [...n.children],
            ...(n.step && volatile.includes(n.step.kind) ? { milestone: true } : {}),
          }))
        );
        setCurrent(saved!.current);
      }),
  };
}
