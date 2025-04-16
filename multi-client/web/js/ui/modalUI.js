// Handles logic for all modals (Model, Servers, Config Editor)

import * as appState from '../state/appState.js';
import { escapeHtml } from '../utils/helpers.js';
import { showToast } from './toast.js';
import * as apiClient from '../api/apiClient.js'; // Import API client
import { renderProvidersList } from './sidebarUI.js'; // To update sidebar on provider change

// --- DOM Elements ---
// Model Modal
let modelModal, modelCloseBtn, providerSelect, modelSelect, apiKeyInput, toggleApiKeyVisibilityBtn, cancelModelChangeBtn, applyModelChangeBtn;
// Servers Modal
let serversModal, serversCloseBtn, serverListItems, addServerBtn, serverForm, noServerSelected, serverNameInput, serverCommandInput, serverArgsList, serverEnvList, addArgBtn, addEnvBtn, cancelServersChangeBtn, applyServersChangeBtn;
// Config Modal
let configModal, configCloseBtn, configFileNameElement, configEditor, cancelConfigChangeBtn, applyConfigChangeBtn;

export function init() {
    // Model Modal Elements
    modelModal = document.getElementById('model-modal');
    modelCloseBtn = modelModal?.querySelector('.close');
    providerSelect = document.getElementById('provider-select');
    modelSelect = document.getElementById('model-select');
    apiKeyInput = document.getElementById('api-key-input');
    toggleApiKeyVisibilityBtn = document.getElementById('toggle-api-key-visibility');
    cancelModelChangeBtn = document.getElementById('cancel-model-change');
    applyModelChangeBtn = document.getElementById('apply-model-change');

    // Servers Modal Elements
    serversModal = document.getElementById('servers-modal');
    serversCloseBtn = serversModal?.querySelector('.close');
    serverListItems = document.getElementById('server-list-items');
    addServerBtn = document.getElementById('add-server-btn');
    serverForm = document.getElementById('server-form');
    noServerSelected = document.getElementById('no-server-selected');
    serverNameInput = document.getElementById('server-name');
    serverCommandInput = document.getElementById('server-command');
    serverArgsList = document.getElementById('server-args');
    serverEnvList = document.getElementById('server-env');
    addArgBtn = document.getElementById('add-arg-btn');
    addEnvBtn = document.getElementById('add-env-btn');
    cancelServersChangeBtn = document.getElementById('cancel-servers-change');
    applyServersChangeBtn = document.getElementById('apply-servers-change');

    // Config Modal Elements
    configModal = document.getElementById('config-modal');
    configCloseBtn = configModal?.querySelector('.close');
    configFileNameElement = document.getElementById('config-file-name');
    configEditor = document.getElementById('config-editor');
    cancelConfigChangeBtn = document.getElementById('cancel-config-change');
    applyConfigChangeBtn = document.getElementById('apply-config-change');

    // Add common modal listeners
    [modelModal, serversModal, configModal].forEach(modal => {
        if (modal) {
            // Close on clicking outside content
            modal.addEventListener('click', (e) => {
                if (e.target === modal) {
                    closeModal(modal);
                }
            });
            // Close button listener
            const closeBtn = modal.querySelector('.close');
            if (closeBtn) {
                closeBtn.addEventListener('click', () => closeModal(modal));
            }
        }
    });

    // --- Model Modal Specific Listeners ---
    if (cancelModelChangeBtn) cancelModelChangeBtn.addEventListener('click', () => closeModal(modelModal));
    if (applyModelChangeBtn) applyModelChangeBtn.addEventListener('click', applyModelChange);
    if (providerSelect) providerSelect.addEventListener('change', updateModelSelectOptions);
    if (toggleApiKeyVisibilityBtn) toggleApiKeyVisibilityBtn.addEventListener('click', toggleApiKeyVisibility);

    // --- Servers Modal Specific Listeners ---
    if (cancelServersChangeBtn) cancelServersChangeBtn.addEventListener('click', () => closeModal(serversModal));
    if (applyServersChangeBtn) applyServersChangeBtn.addEventListener('click', saveServerConfigurations);
    if (addServerBtn) addServerBtn.addEventListener('click', addNewServer);
    if (addArgBtn) addArgBtn.addEventListener('click', () => addArgItem(''));
    if (addEnvBtn) addEnvBtn.addEventListener('click', () => addEnvItem('', ''));

    // --- Config Modal Specific Listeners ---
    if (cancelConfigChangeBtn) cancelConfigChangeBtn.addEventListener('click', () => closeModal(configModal));
    if (applyConfigChangeBtn) applyConfigChangeBtn.addEventListener('click', saveConfigFile);

    console.log("Modal UI initialized.");
}

// --- Common Modal Functions ---
function openModal(modalElement) {
    if (modalElement) {
        modalElement.classList.remove('hidden'); // Use Tailwind hidden class
        // Add class to body to prevent scrolling behind modal?
        // document.body.classList.add('overflow-hidden');
    }
}

function closeModal(modalElement) {
    if (modalElement) {
        modalElement.classList.add('hidden'); // Use Tailwind hidden class
        // Remove body class
        // document.body.classList.remove('overflow-hidden');
    }
}

// --- Model Change Modal Functions ---
export function openModelModal() {
    appState.setStatus('Loading provider data...');
    apiClient.fetchProviders().then(data => {
        // Update state first
        appState.setProviders(data.providers);
        appState.setProviderModels(data.models);
        appState.setCurrentProvider(data.current); // Ensure state has latest current provider

        populateProviderSelect();
        if (apiKeyInput) apiKeyInput.value = ''; // Clear API key field
        openModal(modelModal);
        appState.setStatus('Ready');
    }).catch(err => {
        appState.setStatus('Error loading providers');
        // Toast is shown by apiClient
    });
}

export function closeModelModal() {
    if (apiKeyInput) apiKeyInput.value = ''; // Clear API key for security
    closeModal(modelModal);
}

function populateProviderSelect() {
    if (!providerSelect) return;
    providerSelect.innerHTML = '';
    const providers = appState.getProviders();
    const currentProvider = appState.getCurrentProvider();
    const sortedNames = Object.keys(providers).sort();

    for (const name of sortedNames) {
        const option = document.createElement('option');
        option.value = name;
        option.textContent = name;
        if (name === currentProvider) {
            option.selected = true;
        }
        providerSelect.appendChild(option);
    }
    updateModelSelectOptions(); // Update models for the initially selected provider
}

function updateModelSelectOptions() {
    if (!providerSelect || !modelSelect) return;
    const selectedProviderName = providerSelect.value;
    const providerKey = selectedProviderName.toLowerCase();
    const providerModels = appState.getProviderModels();
    const providers = appState.getProviders();
    const currentModelForProvider = providers[selectedProviderName]?.model;

    modelSelect.innerHTML = '';
    const models = providerModels[providerKey]?.models || [];

    if (models.length === 0) {
        // If no suggestions, check if a model is set in the config
        if (currentModelForProvider) {
             const option = document.createElement('option');
             option.value = currentModelForProvider;
             option.textContent = `${currentModelForProvider} (from config)`;
             option.selected = true;
             modelSelect.appendChild(option);
        } else {
            const option = document.createElement('option');
            option.value = '';
            option.textContent = 'No models available/suggested';
            option.disabled = true;
            modelSelect.appendChild(option);
        }
        return;
    }

    // Add suggested models
    models.forEach(model => {
        const option = document.createElement('option');
        option.value = model;
        option.textContent = model;
        if (model === currentModelForProvider) {
            option.selected = true;
        }
        modelSelect.appendChild(option);
    });

     // If the configured model wasn't in the suggestions, add it as an option
     if (currentModelForProvider && !models.includes(currentModelForProvider)) {
         const configOption = document.createElement('option');
         configOption.value = currentModelForProvider;
         configOption.textContent = `${currentModelForProvider} (from config)`;
         // Prepend it and select it
         modelSelect.prepend(configOption);
         configOption.selected = true;
     }
}


function toggleApiKeyVisibility() {
    if (!apiKeyInput || !toggleApiKeyVisibilityBtn) return;
    const icon = toggleApiKeyVisibilityBtn.querySelector('i');
    if (apiKeyInput.type === 'password') {
        apiKeyInput.type = 'text';
        if (icon) icon.className = 'fas fa-eye-slash';
    } else {
        apiKeyInput.type = 'password';
        if (icon) icon.className = 'fas fa-eye';
    }
}

function applyModelChange() {
    if (!providerSelect || !modelSelect || !apiKeyInput) return;

    const provider = providerSelect.value;
    const model = modelSelect.value;
    const apiKey = apiKeyInput.value.trim();
    const currentProvider = appState.getCurrentProvider();
    const providers = appState.getProviders();
    const currentModel = providers[currentProvider]?.model;

    if (!provider) {
        showToast('warning', 'Warning', 'Please select a provider.');
        return;
    }
     if (!model) {
        showToast('warning', 'Warning', 'Please select a model.');
        return;
    }


    const providerChanged = provider !== currentProvider;
    const modelChanged = model !== currentModel;
    const apiKeyProvided = apiKey !== '';

    if (!providerChanged && !modelChanged && !apiKeyProvided) {
        showToast('info', 'Info', 'No changes to apply.');
        closeModelModal();
        return;
    }

    appState.setStatus('Applying changes...');

    const applyChanges = async () => {
        try {
            // 1. Update API Key if provided
            if (apiKeyProvided) {
                await apiClient.updateApiKey(provider, apiKey);
                // API key update might trigger client switch via socket event if it's the current provider
            }

            // 2. Switch provider/model if changed (and API key update didn't already trigger it)
            const currentProviderAfterKeyUpdate = appState.getCurrentProvider(); // Get potentially updated provider
            if (provider !== currentProviderAfterKeyUpdate || model !== appState.getProviders()[provider]?.model) {
                 await apiClient.switchProviderAndModel(provider, model);
                 // Success toast/UI updates handled by socket event
            }

            closeModelModal(); // Close modal on success
            // Don't reset status here, let socket events handle final state

        } catch (error) {
            // Error toasts are shown by apiClient
            appState.setStatus('Error applying changes');
        }
    };

    applyChanges();
}


// --- Server Configuration Modal Functions ---
export function openServersModal() {
    appState.setStatus('Loading server configurations...');
    apiClient.fetchServerConfig().then(config => {
        appState.setServerConfig(config); // Store the full config for editing
        // Fetch current server status separately if needed, or assume config is enough for now
        // For simplicity, we'll render based on config keys, status display needs separate fetch/socket update
        const serverNames = Object.keys(config.mcpServers || {});
        // Create dummy status data for rendering list
        const serversData = serverNames.map(name => ({ name, status: 'unknown' }));
        renderServerList(serversData); // Render list
        hideServerForm(); // Ensure form is hidden initially
        openModal(serversModal);
        appState.setStatus('Ready');
    }).catch(err => {
        appState.setStatus('Error loading server config');
        // Toast shown by apiClient
    });
}

export function closeServersModal() {
    appState.setSelectedServerName(null); // Clear selection on close
    closeModal(serversModal);
}

function renderServerList(serversData) { // Expects [{ name, status }]
    if (!serverListItems) return;
    serverListItems.innerHTML = '';
    const selectedName = appState.getSelectedServerName();

    if (!serversData || serversData.length === 0) {
        serverListItems.innerHTML = '<li class="server-list-item empty">No servers configured</li>';
        return;
    }

    // Sort by name
    serversData.sort((a, b) => a.name.localeCompare(b.name));

    serversData.forEach(serverInfo => {
        const serverName = serverInfo.name;
        // Status might be unknown if just loaded from config
        const status = serverInfo.status || 'unknown';
        const statusTitle = status.charAt(0).toUpperCase() + status.slice(1);

        const item = document.createElement('li');
        item.className = `server-list-item ${selectedName === serverName ? 'active' : ''}`;
        item.dataset.serverName = serverName;

        // Status indicator (use grey for unknown)
        const statusIndicator = document.createElement('span');
        statusIndicator.className = `server-status-indicator server-status-${status === 'unknown' ? 'disconnected' : status}`; // Map unknown to disconnected style
        statusIndicator.title = statusTitle;

        const nameSpan = document.createElement('span');
        nameSpan.textContent = serverName;

        const deleteBtn = document.createElement('button');
        deleteBtn.className = 'server-delete-btn';
        deleteBtn.innerHTML = '<i class="fas fa-trash"></i>';
        deleteBtn.title = 'Delete server configuration';

        const leftSide = document.createElement('div');
        leftSide.style.display = 'flex';
        leftSide.style.alignItems = 'center';
        leftSide.appendChild(statusIndicator);
        leftSide.appendChild(nameSpan);

        item.appendChild(leftSide);
        item.appendChild(deleteBtn);

        // Select server on click (excluding delete button)
        item.addEventListener('click', (e) => {
            if (!e.target.closest('.server-delete-btn')) {
                selectServer(serverName);
            }
        });

        // Delete button listener
        deleteBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            if (confirm(`Delete server "${serverName}" configuration? Restart required after saving.`)) {
                deleteServer(serverName);
            }
        });

        serverListItems.appendChild(item);
    });
}


function selectServer(serverName) {
    appState.setSelectedServerName(serverName);

    // Update UI list selection
    document.querySelectorAll('.server-list-item').forEach(item => {
        item.classList.toggle('active', item.dataset.serverName === serverName);
    });

    // Populate and show form
    populateServerForm(serverName);
    showServerForm();
}

function populateServerForm(serverName) {
     if (!serverNameInput || !serverCommandInput || !serverArgsList || !serverEnvList) return;

     const config = appState.getServerConfig();
     const serverData = config?.mcpServers?.[serverName];

     if (serverData) {
        serverNameInput.value = serverName;
        serverCommandInput.value = serverData.command || '';

        // Populate args
        serverArgsList.innerHTML = ''; // Clear previous
        (serverData.args || []).forEach(arg => addArgItem(arg));

        // Populate env vars
        serverEnvList.innerHTML = ''; // Clear previous
        Object.entries(serverData.env || {}).forEach(([key, value]) => addEnvItem(key, value));
     } else {
         // Should not happen if selecting from list, but handle defensively
         console.warn(`Config data not found for selected server: ${serverName}`);
         hideServerForm();
     }
}

function showServerForm() {
    if (serverForm) serverForm.classList.remove('hidden'); // Use Tailwind hidden
    if (noServerSelected) noServerSelected.classList.add('hidden'); // Use Tailwind hidden
}

function hideServerForm() {
    if (serverForm) serverForm.classList.add('hidden'); // Use Tailwind hidden
    if (noServerSelected) noServerSelected.classList.remove('hidden'); // Use Tailwind hidden
    // Optionally clear form fields
    if (serverNameInput) serverNameInput.value = '';
    if (serverCommandInput) serverCommandInput.value = '';
    if (serverArgsList) serverArgsList.innerHTML = '';
    if (serverEnvList) serverEnvList.innerHTML = '';
}

function addNewServer() {
    let config = appState.getServerConfig();
    if (!config.mcpServers) config.mcpServers = {}; // Ensure object exists

    let newName = 'new-server';
    let i = 1;
    while (config.mcpServers[newName]) {
        newName = `new-server-${i++}`;
    }

    // Add basic config
    config.mcpServers[newName] = { command: '', args: [], env: {} };
    appState.setServerConfig(config); // Update state

    // Re-render list and select the new server
    const serverNames = Object.keys(config.mcpServers);
    const serversData = serverNames.map(name => ({ name, status: 'unknown' }));
    renderServerList(serversData);
    selectServer(newName);
}

function deleteServer(serverName) {
    let config = appState.getServerConfig();
    if (config?.mcpServers?.[serverName]) {
        delete config.mcpServers[serverName];
        appState.setServerConfig(config); // Update state

        // Re-render list
        const serverNames = Object.keys(config.mcpServers);
        const serversData = serverNames.map(name => ({ name, status: 'unknown' }));
        renderServerList(serversData);

        // If the deleted server was selected, hide form
        if (appState.getSelectedServerName() === serverName) {
            appState.setSelectedServerName(null);
            hideServerForm();
        }
    }
}

function addArgItem(value = '') {
    if (!serverArgsList) return;
    const item = document.createElement('div');
    // Use Tailwind classes for layout and styling
    item.className = 'flex items-center gap-2';
    item.innerHTML = `
        <input type="text" class="form-input flex-grow rounded-md text-sm" value="${escapeHtml(value)}" placeholder="Argument value">
        <button class="remove-btn btn-icon text-danger hover:bg-danger/10 p-1" title="Remove argument"><i class="fas fa-times"></i></button>
    `;
    item.querySelector('.remove-btn').addEventListener('click', () => item.remove());
    serverArgsList.appendChild(item);
}

function addEnvItem(key = '', value = '') {
     if (!serverEnvList) return;
    const item = document.createElement('div');
    // Use Tailwind classes for layout and styling
    item.className = 'flex items-center gap-2';
    item.innerHTML = `
        <input type="text" class="form-input w-1/3 rounded-md text-sm" value="${escapeHtml(key)}" placeholder="Key">
        <input type="text" class="form-input flex-grow rounded-md text-sm" value="${escapeHtml(value)}" placeholder="Value">
        <button class="remove-btn btn-icon text-danger hover:bg-danger/10 p-1" title="Remove variable"><i class="fas fa-times"></i></button>
    `;
    item.querySelector('.remove-btn').addEventListener('click', () => item.remove());
    serverEnvList.appendChild(item);
}

function updateSelectedServerConfigFromForm() {
    const oldServerName = appState.getSelectedServerName();
    if (!oldServerName || !serverNameInput || !serverCommandInput || !serverArgsList || !serverEnvList) return false;

    const newServerName = serverNameInput.value.trim();
    const command = serverCommandInput.value.trim();

    if (!newServerName || !command) {
        showToast('error', 'Error', 'Server name and command are required.');
        return false;
    }

    const args = Array.from(serverArgsList.querySelectorAll('.arg-input'))
                      .map(input => input.value.trim())
                      .filter(Boolean); // Remove empty args

    const env = {};
    serverEnvList.querySelectorAll('.env-item').forEach(item => {
        const key = item.querySelector('.key-input')?.value.trim();
        const value = item.querySelector('.value-input')?.value.trim();
        if (key) env[key] = value || ''; // Allow empty values
    });

    let config = appState.getServerConfig();
    const serverData = { command, args, env };

    // Handle rename: remove old, add new
    if (oldServerName !== newServerName) {
        delete config.mcpServers[oldServerName];
    }
    config.mcpServers[newServerName] = serverData;

    appState.setServerConfig(config); // Update state
    appState.setSelectedServerName(newServerName); // Update selected name if renamed

    return true; // Indicate success
}


function saveServerConfigurations() {
    // Update the currently selected server's data from the form first
    if (appState.getSelectedServerName()) {
        if (!updateSelectedServerConfigFromForm()) {
            return; // Validation failed in update function
        }
    }

    // Get the potentially updated config from state
    const configToSave = appState.getServerConfig();

    appState.setStatus('Saving server configurations...');
    apiClient.saveServerConfigurations(configToSave).then(success => {
        if (success) {
            closeServersModal();
            // Toast shown by apiClient
        }
        appState.setStatus('Ready'); // Reset status eventually
    });
}


// --- Config Editor Modal Functions ---
export function openConfigEditor(fileName) {
    if (!fileName || !configFileNameElement || !configEditor) return;

    appState.setCurrentConfigFile(fileName);
    configFileNameElement.textContent = fileName;
    configEditor.value = 'Loading...';
    configEditor.disabled = true;
    appState.setStatus(`Loading ${fileName}...`);
    openModal(configModal);

    apiClient.fetchConfigFile(fileName).then(data => {
        let content = data.content;
        // Pretty print JSON
        if (fileName.endsWith('.json')) {
            try {
                content = JSON.stringify(JSON.parse(content), null, 2);
            } catch { /* Ignore parse error, show raw */ }
        }
        configEditor.value = content;
        configEditor.disabled = false;
        appState.setStatus('Ready');
    }).catch(err => {
        configEditor.value = `Error loading ${fileName}:\n${err.message}`;
        appState.setStatus('Error loading config');
        // Optionally close modal on error, or let user close manually
        // closeModal(configModal);
    });
}

export function closeConfigModal() {
    appState.setCurrentConfigFile(null);
    if (configEditor) configEditor.value = ''; // Clear editor
    closeModal(configModal);
}

function saveConfigFile() {
    const fileName = appState.getCurrentConfigFile();
    if (!fileName || !configEditor) return;

    const content = configEditor.value;

    // Basic validation (more specific validation in API client)
    if (fileName.endsWith('.json')) {
        try { JSON.parse(content); } catch (e) {
            showToast('error', 'Invalid JSON', e.message); return;
        }
    }
     if (fileName.endsWith('.toml')) {
        try {
            // Basic TOML validation might be complex client-side, rely on server/API validation
        } catch (e) {
             showToast('error', 'Invalid TOML', e.message); return;
        }
    }


    appState.setStatus(`Saving ${fileName}...`);
    apiClient.saveConfigFile(fileName, content).then(data => {
        if (data.success) {
            closeConfigModal();
            // Toast shown by apiClient
            // If servers.json was saved, maybe refresh server list/status?
            if (fileName === 'servers.json') {
                // Potentially trigger a refresh of server data/status display
                console.log("servers.json saved, consider refreshing server status display.");
            }
             if (fileName === 'ai_config.json') {
                 // Refresh provider data
                 apiClient.fetchProviders().then(data => {
                     appState.setProviders(data.providers);
                     appState.setProviderModels(data.models);
                     appState.setCurrentProvider(data.current);
                     renderProvidersList(); // Re-render sidebar
                 });
             }
        }
        appState.setStatus('Ready');
    }).catch(err => {
        // Error toast shown by apiClient
        appState.setStatus('Error saving config');
    });
}
