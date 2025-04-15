// Holds shared application state

let state = {
    isThinking: false,
    currentProvider: '',
    providers: {}, // { providerName: { config } }
    providerModels: {}, // { providerName: { models: [] } }
    serverConfig: { mcpServers: {} }, // For server modal editing
    selectedServerName: null, // For server modal editing
    conversations: [], // [{ id, title, ... }]
    currentConversationId: null,
    allToolsData: {}, // { serverName: [tools] }
    statusMessage: 'Initializing...',
    currentConfigFile: null, // For config editor modal
};

export function initState() {
    // Perform any initial setup if needed
    console.log("App state initialized");
}

// --- Getters ---
export function isThinking() { return state.isThinking; }
export function getCurrentProvider() { return state.currentProvider; }
export function getProviders() { return state.providers; }
export function getProviderModels() { return state.providerModels; }
export function getServerConfig() { return state.serverConfig; }
export function getSelectedServerName() { return state.selectedServerName; }
export function getConversations() { return state.conversations; }
export function getCurrentConversationId() { return state.currentConversationId; }
export function getAllToolsData() { return state.allToolsData; }
export function getStatusMessage() { return state.statusMessage; }
export function getCurrentConfigFile() { return state.currentConfigFile; }

// --- Setters ---
export function setThinking(thinking) {
    state.isThinking = thinking;
    // Optionally trigger UI updates here or let callers handle it
}
export function setCurrentProvider(provider) { state.currentProvider = provider; }
export function setProviders(providersData) { state.providers = providersData || {}; }
export function setProviderModels(modelsData) { state.providerModels = modelsData || {}; }
export function setServerConfig(config) { state.serverConfig = config || { mcpServers: {} }; }
export function setSelectedServerName(name) { state.selectedServerName = name; }
export function setConversations(convos) { state.conversations = convos || []; }
export function setCurrentConversationId(id) { state.currentConversationId = id; }
export function setAllToolsData(tools) { state.allToolsData = tools || {}; }
export function setStatus(message) {
    state.statusMessage = message;
    // Update the status UI element directly (example)
    const statusElement = document.getElementById('status');
    if (statusElement) {
        statusElement.textContent = message;
    }
}
export function setCurrentConfigFile(fileName) { state.currentConfigFile = fileName; }

// --- Updaters ---
export function updateConversationInList(updatedConversation) {
    const index = state.conversations.findIndex(c => c.id === updatedConversation.id);
    if (index >= 0) {
        state.conversations[index] = updatedConversation;
    } else {
        state.conversations.push(updatedConversation);
    }
    // Sort again after update/add
    state.conversations.sort((a, b) => new Date(b.updatedAt) - new Date(a.updatedAt));
}

export function removeConversationFromList(conversationId) {
     state.conversations = state.conversations.filter(c => c.id !== conversationId);
}
