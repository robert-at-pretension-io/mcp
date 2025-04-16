// Handles rendering and interactions for the right sidebar panels

import { escapeHtml, formatRelativeTime } from '../utils/helpers.js';
import * as appState from '../state/appState.js';
import { emitNewConversation, emitLoadConversation } from '../socket/socketClient.js';
import * as apiClient from '../api/apiClient.js'; // Import the whole module
import { openConfigEditor } from './modalUI.js'; // Import modal function
import { showToast } from './toast.js'; // Import toast for errors

// DOM Elements
let rightPanel; // The actual panel element
let mainElement; // Main content element
let toggleRightPanelBtn; // Button in header
let closeRightPanelBtn; // Button inside panel (for mobile)
let conversationsListElement;
let newConversationBtn;
let providersListElement;
let toolsListElement;
let toolFilterInput;
let editConfigsBtn;
let configOptionButtons;
let panelSections; // To store all panel sections

// Initialize UI
export function init() {
    rightPanel = document.getElementById('right-panel');
    mainElement = document.querySelector('main');
    toggleRightPanelBtn = document.getElementById('toggle-right-panel');
    closeRightPanelBtn = document.getElementById('close-right-panel-btn');
    conversationsListElement = document.getElementById('conversations-list');
    newConversationBtn = document.getElementById('new-conversation-btn');
    providersListElement = document.getElementById('providers-list');
    toolsListElement = document.getElementById('tools-list');
    toolFilterInput = document.getElementById('tool-filter-input');
    editConfigsBtn = document.getElementById('edit-configs-btn');
    configOptionButtons = document.querySelectorAll('.config-option');
    panelSections = document.querySelectorAll('.right-panel .panel-section'); // Get all sections

    if (!rightPanel || !mainElement || !toggleRightPanelBtn || !conversationsListElement || !newConversationBtn || !providersListElement || !toolsListElement || !toolFilterInput || !editConfigsBtn || !panelSections) {
        console.error("Sidebar UI elements not found!");
        return;
    }

    // Event Listeners
    toggleRightPanelBtn.addEventListener('click', toggleRightPanel);
    if (closeRightPanelBtn) {
        closeRightPanelBtn.addEventListener('click', toggleRightPanel); // Close button also toggles
    }
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

    // Accordion listeners
    panelSections.forEach(section => {
        const header = section.querySelector('h3');
        if (header) {
            header.addEventListener('click', () => {
                toggleAccordionSection(section);
            });
        }
    });

    // Set initial state (e.g., open Conversations by default)
    const initialSection = document.querySelector('.conversations-panel');
    if (initialSection) {
        initialSection.classList.add('active');
    }

    console.log("Sidebar UI initialized.");
}

function toggleAccordionSection(sectionToToggle) {
    panelSections.forEach(section => {
        if (section === sectionToToggle) {
            section.classList.toggle('active'); // Toggle the clicked section
        } else {
            section.classList.remove('active'); // Close all other sections
        }
    });
}

// Toggle right panel visibility
function toggleRightPanel() {
    if (!rightPanel || !mainElement) return;
    
    const isOpen = rightPanel.classList.contains('open');
    rightPanel.classList.toggle('open');
    
    // Optional: Also toggle a class on main element for responsive layouts
    if (mainElement) {
        mainElement.classList.toggle('panel-open', !isOpen);
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
        // Safely parse date, provide fallback
        let updatedDate = new Date(conversation.updatedAt);
        let formattedDate = 'Invalid date';
        let relativeTime = 'unknown';

        if (!isNaN(updatedDate.getTime())) {
            formattedDate = updatedDate.toLocaleDateString() + ' ' + updatedDate.toLocaleTimeString();
            relativeTime = formatRelativeTime(updatedDate);
        } else {
            // Handle cases where updatedAt might be missing or invalid
            console.warn(`Invalid updatedAt date for conversation ${conversation.id}:`, conversation.updatedAt);
            // Optionally try createdAt or provide a default
            updatedDate = new Date(conversation.createdAt); // Try createdAt
             if (!isNaN(updatedDate.getTime())) {
                 formattedDate = updatedDate.toLocaleDateString() + ' ' + updatedDate.toLocaleTimeString();
                 relativeTime = formatRelativeTime(updatedDate);
             } else {
                 formattedDate = 'Date unavailable';
                 relativeTime = 'unknown time';
             }
        }


        const activeClasses = isActive ? 'bg-primary/10 dark:bg-primary/20 border-primary' : 'border-transparent hover:bg-gray-100 dark:hover:bg-gray-700';
        html += `
            <div class="conversation-item p-3 rounded-lg border ${activeClasses} cursor-pointer transition-colors group relative" data-id="${escapeHtml(conversation.id)}">
                <div class="font-medium text-sm truncate mb-1">${escapeHtml(conversation.title || 'Untitled Conversation')}</div>
                <div class="text-xs text-gray-500 dark:text-gray-400 flex justify-between items-center">
                    <span class="truncate" title="${escapeHtml(conversation.provider || '')} - ${escapeHtml(conversation.modelName || '')}">
                        ${escapeHtml(conversation.provider?.substring(0, 8) || 'N/A')} - ${escapeHtml(conversation.modelName?.substring(0, 12) || 'N/A')}
                    </span>
                    <span class="conversation-date flex-shrink-0 ml-2" title="${formattedDate}">${relativeTime}</span>
                </div>
                <div class="conversation-actions absolute top-1 right-1 opacity-0 group-hover:opacity-100 transition-opacity flex gap-1">
                    <button class="conversation-rename-btn btn-icon p-1 text-xs" title="Rename"><i class="fas fa-edit"></i></button>
                    <button class="conversation-delete-btn btn-icon p-1 text-xs text-danger hover:bg-danger/10" title="Delete"><i class="fas fa-trash"></i></button>
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
        const activeClasses = isActive ? 'bg-primary/10 dark:bg-primary/20 border-primary' : 'border-transparent hover:bg-gray-100 dark:hover:bg-gray-700';
        html += `
            <div class="provider-item p-3 rounded-lg border ${activeClasses} cursor-pointer transition-colors" data-provider="${escapeHtml(name)}">
                <div class="flex justify-between items-center mb-1">
                    <span class="font-medium text-sm">${escapeHtml(name)}</span>
                    ${isActive ? '<i class="fas fa-check-circle text-success" title="Active Provider"></i>' : ''}
                </div>
                <div class="text-xs text-gray-500 dark:text-gray-400 truncate" title="${escapeHtml(config.model || '')}">
                    Model: ${escapeHtml(config.model || 'Default')}
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
            html += `<div class="tool-server-group mb-4 last:mb-0">
                        <h4 class="text-sm font-semibold mb-2 text-gray-600 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700 pb-1">${escapeHtml(serverName)}</h4>
                        <div class="space-y-2">`;
            filteredTools.forEach(tool => {
                html += `
                    <div class="tool-item p-2 rounded bg-gray-50 dark:bg-gray-700/50">
                        <h5 class="text-sm font-medium">${escapeHtml(tool.name)}</h5>
                        <p class="text-xs text-gray-600 dark:text-gray-400">${escapeHtml(tool.description || 'No description')}</p>
                    </div>
                `;
            });
            html += `   </div>
                     </div>`;
        }
    }

    const emptyListClasses = "text-center text-sm text-gray-500 dark:text-gray-400 py-4";
    if (!foundTools) {
        if (Object.keys(toolsByServer || {}).length === 0) {
            html = `<div class="${emptyListClasses}">No tools available from connected servers.</div>`;
        } else if (filterText) {
            html = `<div class="${emptyListClasses}">No tools match filter "${escapeHtml(filterText)}".</div>`;
        } else {
             html = `<div class="${emptyListClasses}">No tools found.</div>`;
        }
    }

    toolsListElement.innerHTML = html;
}
