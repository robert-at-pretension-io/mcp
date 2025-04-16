// Handles updates to the header elements (server count, model name)

import { openModelModal } from './modalUI.js'; // To open modals
import { openServersModal } from './modalUI.js';

// DOM Elements
// DOM Elements
let connectedServersElement;
let aiModelElement;
let changeModelBtn;
let manageServersBtn;
// No need for toggle button reference here, handled in sidebarUI

export function init() {
    connectedServersElement = document.getElementById('connected-servers');
    aiModelElement = document.getElementById('ai-model');
    changeModelBtn = document.getElementById('change-model-btn');
    manageServersBtn = document.getElementById('manage-servers-btn');

    if (!connectedServersElement || !aiModelElement || !changeModelBtn || !manageServersBtn) {
        console.error("Header UI elements not found!");
        return;
    }

    // Event Listeners
    changeModelBtn.addEventListener('click', openModelModal);
    manageServersBtn.addEventListener('click', openServersModal);
    // Toggle button listener moved to sidebarUI

    console.log("Header UI initialized.");
}

// Update server connection info display
export function updateServerInfo(serversData) { // Expects array like [{ name, status }] or just names
    if (!connectedServersElement) return;

    let count = 0;
    let statusText = 'No servers connected';

    if (Array.isArray(serversData)) {
        // If just names are passed (backward compatibility or simple status)
        if (serversData.length > 0 && typeof serversData[0] === 'string') {
            count = serversData.length;
            statusText = `${count} server${count !== 1 ? 's' : ''} connected`;
        }
        // If status objects are passed (preferred)
        else if (serversData.length > 0 && typeof serversData[0] === 'object') {
            const connectedCount = serversData.filter(s => s.status === 'connected').length;
            const errorCount = serversData.filter(s => s.status === 'error').length;
            count = connectedCount; // Display connected count primarily

            statusText = `${connectedCount} server${connectedCount !== 1 ? 's' : ''} connected`;
            if (errorCount > 0) {
                statusText += ` (${errorCount} error${errorCount !== 1 ? 's' : ''})`;
            } else if (connectedCount === 0 && serversData.length > 0) {
                statusText = 'No servers connected';
            } else if (serversData.length === 0) {
                statusText = 'No servers configured';
            }
        }
    }

    connectedServersElement.textContent = statusText;
}

// Update AI model display in header
export function updateModelInfo(provider, model) {
    if (!aiModelElement) return;
    if (provider && model) {
        aiModelElement.textContent = `${model} (${provider})`;
        aiModelElement.title = `Provider: ${provider}, Model: ${model}`;
    } else if (model) {
         aiModelElement.textContent = model;
         aiModelElement.title = `Model: ${model}`;
    }
     else {
        aiModelElement.textContent = 'N/A';
        aiModelElement.title = 'No AI model selected or available';
    }
}
