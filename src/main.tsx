import React, { Suspense, lazy } from 'react';
import ReactDOM from 'react-dom/client';
import { getStandaloneTabParams, getStandaloneWindowId } from './lib/windowControlsBridge';
import './styles/globals.css';

const App = lazy(() => import('./App'));
const StandaloneWindowHost = lazy(() =>
  import('./components/layout/StandaloneWindowHost').then((m) => ({
    default: m.StandaloneWindowHost,
  }))
);

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('Failed to find the root element');
}

const standaloneTool = getStandaloneWindowId();
const standaloneTab = getStandaloneTabParams();

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <Suspense
      fallback={
        <div className="flex h-screen w-screen items-center justify-center bg-kiviBg font-mono text-xs text-kiviMuted">
          Loading kiviTuner...
        </div>
      }
    >
      {standaloneTab ? (
        <StandaloneWindowHost tabParams={standaloneTab} />
      ) : standaloneTool ? (
        <StandaloneWindowHost toolId={standaloneTool} />
      ) : (
        <App />
      )}
    </Suspense>
  </React.StrictMode>
);
