import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { getStandaloneWindowId } from './lib/windowControlsBridge';
import { StandaloneWindowHost } from './components/layout/StandaloneWindowHost';
import './styles/globals.css';

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('Failed to find the root element');
}

const standaloneTool = getStandaloneWindowId();

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    {standaloneTool ? <StandaloneWindowHost toolId={standaloneTool} /> : <App />}
  </React.StrictMode>
);
