// Web Worker: hornea clips y saca límites observados sin congelar la interfaz
import { runAnimJob, type AnimJob } from "./animWorkerCore";

type Incoming = { id: number; job: AnimJob };

// Aviso de arranque: el cliente lo espera unos segundos y, si no llega, cae al respaldo síncrono
self.postMessage({ ready: true });

self.onmessage = (ev: MessageEvent<Incoming>) => {
  const { id, job } = ev.data;
  try {
    const result = runAnimJob(job, (done, total) => self.postMessage({ id, progress: { done, total } }));
    self.postMessage({ id, result });
  } catch (e) {
    self.postMessage({ id, error: e instanceof Error ? e.message : String(e) });
  }
};
