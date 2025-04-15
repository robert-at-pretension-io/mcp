// Main entry point for the frontend application

import * as appState from './state/appState.js';
import * as apiClient from './api/apiClient.js';
import * as socketClient from './socket/socketClient.js';
import * as chatUI from './ui/chatUI.js';
import * as sidebarUI from './ui/sidebarUI.js';
import * as modalUI from './ui/modalUI.js';
import * as headerUI from './ui/headerUI.js';
import { showToast } from './ui/toast.js';

// --- Initialization ---
function initializeApp() {
    console.log("Initializing application modules...");
    appState.setStatus('Initializing...');

    // Initialize state
    appState.initState();

    // Initialize UI components (add event listeners etc.)
    headerUI.init();
    chatUI.init();
    sidebarUI.init();
    modalUI.init();

    // Initialize Socket.IO connection and listeners
    socketClient.init({
        onConnect: () => appState.setStatus('Connected'),
        onDisconnect: (reason) => appState.setStatus(`Disconnected: ${reason}`),
        // Pass UI update functions or state setters to socket handlers
        onServersInfo: headerUI.updateServerInfo,
        onHistory: chatUI.renderConversationHistory,
        onHistoryUpdate: chatUI.renderConversationHistory,
        onThinking: chatUI.updateThinkingIndicator,
        onToolsInfo: (tools) => {
            // Assuming tools is { serverName: [tool] }
            appState.setAllToolsData(tools);
            sidebarUI.renderToolsList(tools); // Pass data directly
        },
        onStatusUpdate: appState.setStatus,
        onError: (message) => {
            // Error toast is shown by socketClient, just update status maybe
            appState.setStatus('Error');
            chatUI.displayError(message); // Also display in chat
        },
        onConversationCleared: () => {
            chatUI.clearConversationDisplay();
            showToast('info', 'Conversation Cleared', 'Started a new chat session.');
        },
        onModelChanged: (data) => {
            // Update state first
            appState.setCurrentProvider(data.provider);
            // Fetch providers again to get potentially updated model in config
            apiClient.fetchProviders().then(pData => {
                appState.setProviders(pData.providers);
                appState.setProviderModels(pData.models);
                // Update header and sidebar
                headerUI.updateModelInfo(data.provider, data.model);
                sidebarUI.renderProvidersList();
            });
            modalUI.closeModelModal(); // Close modal if open
            showToast('success', 'Model Changed', `Switched to ${data.model} (${data.provider})`);
        },
        onConversationsList: (conversations) => {
            appState.setConversations(conversations);
            sidebarUI.renderConversationsList();
        },
        onConversationLoaded: (data) => {
            appState.setCurrentConversationId(data.id);
            sidebarUI.updateConversationSelection();
            chatUI.renderConversationHistory(data.messages);
            appState.setStatus('Ready'); // Set status to ready after loading
        },
        onConversationSaved: (data) => {
            // Toast handled by the action initiator (e.g., after sending message)
            // Update state and UI list
            appState.updateConversationInList(data);
            sidebarUI.renderConversationsList();
            appState.setCurrentConversationId(data.id); // Ensure current ID is set
        },
    });

    // --- Initial Data Fetching ---
    // Fetch conversations first
    appState.setStatus('Loading conversations...');
    apiClient.fetchConversationsList().then(convos => {
        appState.setConversations(convos);
        sidebarUI.renderConversationsList(); // Render initial list

        // Load initial conversation if needed
        const currentId = appState.getCurrentConversationId(); // Check if already loaded via socket
        if (!currentId && convos.length > 0) {
            const sortedConversations = [...convos].sort((a, b) => {
                return new Date(b.updatedAt) - new Date(a.updatedAt);
            });
            if (sortedConversations[0]) {
                console.log(`Loading most recent conversation: ${sortedConversations[0].id}`);
                socketClient.emitLoadConversation(sortedConversations[0].id);
            } else {
                 appState.setStatus('Ready'); // No conversation to load
            }
        } else if (!currentId) {
             appState.setStatus('Ready'); // No conversations exist
        }
        // If currentId exists, assume socket 'conversation-loaded' handled it

        // Fetch providers after conversations (or in parallel)
        appState.setStatus('Loading AI providers...');
        return apiClient.fetchProviders();
    }).then(data => {
        appState.setProviders(data.providers);
        appState.setProviderModels(data.models);
        appState.setCurrentProvider(data.current);
        sidebarUI.renderProvidersList(); // Initial render
        // Update header model info based on fetched current provider/model
        const currentProviderData = data.providers[data.current];
        headerUI.updateModelInfo(data.current, currentProviderData?.model);
        // Don't set status to Ready here if a conversation is still loading
        if (!appState.getCurrentConversationId() && appState.getConversations().length === 0) {
             appState.setStatus('Ready');
        }
    }).catch(err => {
        console.error("Error during initial data fetch:", err);
        appState.setStatus('Initialization Error');
        // Toasts are shown by apiClient calls
    });

    console.log("Application initialization sequence started.");
}

// --- Run Initialization ---
// Ensure DOM is ready before running initialization
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initializeApp);
} else {
    initializeApp(); // DOMContentLoaded has already fired
}
