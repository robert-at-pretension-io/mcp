// Handles rendering and interactions for the right sidebar panels

import { escapeHtml, formatRelativeTime } from '../utils/helpers.js';
import * as appState from '../state/appState.js';
import { emitNewConversation, emitLoadConversation } from '../socket/socketClient.js';
import * as apiClient from '../api/apiClient.js'; // Import the whole module
import { openConfigEditor } from './modalUI.js'; // Import modal function
import { showToast } from './toast.js'; // Import toast for errors

// DOM Elements
let mainElement;
let toggleRightPanelBtn;
let conversationsListElement;
let newConversationBtn;
let providersListElement;
let toolsListElement;
let toolFilterInput;
let editConfigsBtn;
let configOptionButtons;

export function init() {
    mainElement = document.querySelector('main');
    toggleRightPanelBtn = document.getElementById('toggle-right-panel');
    conversationsListElement = document.getElementById('conversations-list');
    newConversationBtn = document.getElementById('new-conversation-btn');
    providersListElement = document.getElementById('providers-list');
    toolsListElement = document.getElementById('tools-list');
    toolFilterInput = document.getElementById('tool-filter-input');
    editConfigsBtn = document.getElementById('edit-configs-btn');
    configOptionButtons = document.querySelectorAll('.config-option');

    if (!mainElement || !toggleRightPanelBtn || !conversationsListElement || !newConversationBtn || !providersListElement || !toolsListElement || !toolFilterInput || !editConfigsBtn) {
        console.error("Sidebar UI elements not found!");
        return;
    }

    // Event Listeners
    toggleRightPanelBtn.addEventListener('click', toggleRightPanel);
    newConversationBtn.addEventListener('click', handleNewConversation);
    toolFilterInput.addEventListener('input', handleToolFilterChange);
    editConfigsBtn.addEventListener('click', showConfigOptions);

    configOptionButtons.forEach(button => {
        button.addEventListener('click', () => {
            const fileName = button.dataset.file;
            if (fileName) {
                openConfigEditor(fileName); // Call function from modalUI
            }
        });
    });

    console.log("Sidebar UI initialized.");
}

function toggleRightPanel() {
    mainElement.classList.toggle('panel-collapsed');
    // Update icon (optional)
    const icon = toggleRightPanelBtn.querySelector('i');
    if (icon) {
        icon.className = mainElement.classList.contains('panel-collapsed')
            ? 'fas fa-columns' // Icon to show when collapsed (suggests expanding)
            : 'fas fa-times'; // Icon to show when expanded (suggests closing) - Use fa-times or fa-chevron-right
    }
}

function handleNewConversation() {
    emitNewConversation();
    // UI updates (clearing chat, selecting new item) are handled by socket event listeners
}

function handleToolFilterChange() {
    renderToolsList(appState.getAllToolsData()); // Re-render with filter applied
}

function showConfigOptions() {
    // Highlight buttons briefly
    configOptionButtons.forEach(btn => btn.classList.add('highlight'));
    setTimeout(() => {
        configOptionButtons.forEach(btn => btn.classList.remove('highlight'));
    }, 3000);
    // Toast handled in main.js or here if preferred
}

// --- Rendering Functions ---

export function renderConversationsList() {
    if (!conversationsListElement) return;

    const conversations = appState.getConversations(); // Get data from state
    const currentConversationId = appState.getCurrentConversationId();

    if (!conversations || conversations.length === 0) {
        conversationsListElement.innerHTML = '<div class="empty-list">No saved conversations</div>';
        return;
    }

    // Sorting is handled in appState now
    let html = '';
    for (const conversation of conversations) {
        const isActive = conversation.id === currentConversationId;
        // Ensure dates are parsed correctly
        const updatedDate = new Date(conversation.updatedAt || Date.now()); // Fallback to now if date missing
        const formattedDate = updatedDate.toLocaleDateString() + ' ' + updatedDate.toLocaleTimeString();

        html += `
            <div class="conversation-item ${isActive ? 'active' : ''}" data-id="${escapeHtml(conversation.id)}">
                <div class="conversation-title">${escapeHtml(conversation.title || 'Untitled Conversation')}</div>
                <div class="conversation-meta">
                    <span class="conversation-model" title="${escapeHtml(conversation.provider || '')} - ${escapeHtml(conversation.modelName || '')}">
                        ${escapeHtml(conversation.provider?.substring(0, 10) || '')} - ${escapeHtml(conversation.modelName?.substring(0, 15) || '')}
                    </span>
                    <span class="conversation-date" title="${formattedDate}">${formatRelativeTime(updatedDate)}</span>
                </div>
                <div class="conversation-actions">
                    <button class="conversation-rename-btn" title="Rename conversation"><i class="fas fa-edit"></i></button>
                    <button class="conversation-delete-btn" title="Delete conversation"><i class="fas fa-trash"></i></button>
                </div>
            </div>
        `;
    }

    conversationsListElement.innerHTML = html;
    addConversationItemListeners(); // Add listeners after rendering
}

function addConversationItemListeners() {
    document.querySelectorAll('.conversation-item').forEach(item => {
        // Load conversation on click (if not clicking actions)
        item.addEventListener('click', (e) => {
            if (!e.target.closest('.conversation-actions')) {
                const conversationId = item.dataset.id;
                if (conversationId !== appState.getCurrentConversationId()) {
                    emitLoadConversation(conversationId);
                }
            }
        });

        // Rename button
        const renameBtn = item.querySelector('.conversation-rename-btn');
        if (renameBtn) {
            renameBtn.addEventListener('click', (e) => {
                e.stopPropagation(); // Prevent triggering load
                const conversationId = item.dataset.id;
                const conversation = appState.getConversations().find(c => c.id === conversationId);
                // Removed duplicate declaration of 'conversation'
                if (conversation) {
                    const newTitle = prompt('Enter new title:', conversation.title || '');
                    if (newTitle !== null && newTitle.trim() !== (conversation.title || '')) { // Compare with potentially empty title
                        apiClient.renameConversation(conversationId, newTitle.trim()).then(success => { // Use apiClient namespace
                            if (success) {
                                // Update state and re-render (or wait for socket event)
                                appState.updateConversationInList({ ...conversation, title: newTitle.trim(), updatedAt: new Date().toISOString() }); // Update timestamp
                                renderConversationsList();
                            }
                        });
                    }
                }
            });
        }

        // Delete button
        const deleteBtn = item.querySelector('.conversation-delete-btn');
        if (deleteBtn) {
            deleteBtn.addEventListener('click', (e) => {
                e.stopPropagation(); // Prevent triggering load
                const conversationId = item.dataset.id;
                if (confirm('Are you sure you want to delete this conversation?')) {
                    apiClient.deleteConversation(conversationId).then(success => { // Use apiClient namespace
                        if (success) {
                            // Update state and re-render (or wait for socket event)
                            const wasCurrent = appState.getCurrentConversationId() === conversationId;
                            appState.removeConversationFromList(conversationId);
                            renderConversationsList();
                            if (wasCurrent) {
                                // If current was deleted, emit new conversation event
                                emitNewConversation();
                            }
                        }
                    });
                }
            });
        }
    });
}


export function updateConversationSelection() {
    const currentConversationId = appState.getCurrentConversationId();
    document.querySelectorAll('.conversation-item').forEach(item => {
        item.classList.toggle('active', item.dataset.id === currentConversationId);
    });
}

export function renderProvidersList() {
    if (!providersListElement) return;

    const providers = appState.getProviders();
    const currentProvider = appState.getCurrentProvider();

    if (!providers || Object.keys(providers).length === 0) {
        providersListElement.innerHTML = '<div class="empty-list">No AI providers configured</div>';
        return;
    }

    let html = '';
    // Sort providers alphabetically for consistent order
    const sortedProviderNames = Object.keys(providers).sort();

    for (const name of sortedProviderNames) {
        const config = providers[name];
        const isActive = name === currentProvider;
        html += `
            <div class="provider-item ${isActive ? 'active' : ''}" data-provider="${escapeHtml(name)}">
                <div class="provider-name">
                    <span>${escapeHtml(name)}</span>
                    ${isActive ? '<i class="fas fa-check-circle" title="Active Provider"></i>' : ''}
                </div>
                <div class="provider-model" title="${escapeHtml(config.model || '')}">
                    ${escapeHtml(config.model || 'Default model')}
                    ${isActive ? ' (Active)' : ''}
                </div>
            </div>
        `;
    }

    providersListElement.innerHTML = html;
    addProviderItemListeners(); // Add listeners after rendering
}

function addProviderItemListeners() {
     document.querySelectorAll('.provider-item').forEach(item => {
        item.addEventListener('click', () => {
            const providerName = item.dataset.provider;
            if (providerName && providerName !== appState.getCurrentProvider()) {
                // Find the default/first model for this provider to switch
                const providerModels = appState.getProviderModels();
                const models = providerModels[providerName.toLowerCase()]?.models || [];
                const targetModel = appState.getProviders()[providerName]?.model || models[0] || ''; // Use configured or first suggested

                if (targetModel) {
                         appState.setStatus(`Switching to ${providerName}...`);
                         // Use API client to switch
                         apiClient.switchProviderAndModel(providerName, targetModel)
                            .catch(err => appState.setStatus('Ready')); // Reset status on error
                         // UI updates are handled by the 'model-changed' socket event
                    } else {
                        showToast('error', 'Error', `No model configured or suggested for provider ${providerName}. Cannot switch.`);
                }
            }
        });
    });
}


export function renderToolsList(toolsByServer) { // Expects { serverName: [tools] }
    if (!toolsListElement || !toolFilterInput) return;

    const filterText = toolFilterInput.value.toLowerCase().trim();
    let html = '';
    let foundTools = false;
    const serverNames = Object.keys(toolsByServer || {}).sort(); // Sort server names

    for (const serverName of serverNames) {
        const tools = toolsByServer[serverName] || [];
        const filteredTools = tools.filter(tool =>
            tool.name.toLowerCase().includes(filterText) ||
            (tool.description && tool.description.toLowerCase().includes(filterText))
        ).sort((a, b) => a.name.localeCompare(b.name)); // Sort tools alphabetically

        if (filteredTools.length > 0) {
            foundTools = true;
            html += `<div class="tool-server-group"><h4>${escapeHtml(serverName)}</h4>`;
            filteredTools.forEach(tool => {
                html += `
                    <div class="tool-item">
                        <h5>${escapeHtml(tool.name)}</h5>
                        <div class="tool-description">${escapeHtml(tool.description || 'No description')}</div>
                    </div>
                `;
            });
            html += `</div>`;
        }
    }

    if (!foundTools) {
        if (Object.keys(toolsByServer || {}).length === 0) {
            html = '<div class="empty-list">No tools available from connected servers.</div>';
        } else if (filterText) {
            html = `<div class="empty-list">No tools match filter "${escapeHtml(filterText)}".</div>`;
        } else {
             html = '<div class="empty-list">No tools found.</div>';
        }
    }

    toolsListElement.innerHTML = html;
}
