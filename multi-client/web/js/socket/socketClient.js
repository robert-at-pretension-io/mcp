// Handles Socket.IO connection and events

// Import UI update functions or state setters needed by handlers
import { showToast } from '../ui/toast.js';

let socket = null;
let handlers = {}; // Store handlers passed during init

export function init(eventHandlers) {
    if (socket) {
        console.warn("Socket already initialized.");
        return;
    }

    handlers = eventHandlers || {}; // Store the handlers

    console.log("Initializing Socket.IO connection...");
    socket = io(); // Assumes socket.io client is loaded globally

    // --- Connection Events ---
    socket.on('connect', () => {
        console.log('Socket connected:', socket.id);
        if (handlers.onConnect) handlers.onConnect();
        // Request initial data upon connection/reconnection
        // (Server should ideally send this automatically on connect)
    });

    socket.on('disconnect', (reason) => {
        console.log('Socket disconnected:', reason);
        if (handlers.onDisconnect) handlers.onDisconnect(reason);
        showToast('warning', 'Disconnected', `Lost connection to server: ${reason}`);
    });

    socket.on('connect_error', (error) => {
        console.error('Socket connection error:', error);
        if (handlers.onError) handlers.onError(`Connection error: ${error.message}`);
        showToast('error', 'Connection Error', `Could not connect to server: ${error.message}`);
    });

    // --- Application Specific Event Handlers ---
    // These handlers call the functions passed in via `eventHandlers` during init

    socket.on('servers-info', (data) => {
        // console.log('Received servers-info:', data);
        if (handlers.onServersInfo) handlers.onServersInfo(data.servers);
    });

    socket.on('history', (data) => {
        // console.log('Received history:', data);
        if (handlers.onHistory) handlers.onHistory(data.history);
    });

    socket.on('history-update', (data) => {
        // console.log('Received history-update:', data);
        if (handlers.onHistoryUpdate) handlers.onHistoryUpdate(data.history);
    });

    // 'ai-response' might not be needed if history-update covers it
    // socket.on('ai-response', (data) => {
    //     console.log('Received ai-response:', data);
    //     // Handle if needed, maybe just log or confirm message receipt
    // });

    socket.on('thinking', (data) => {
        // console.log('Received thinking:', data);
        if (handlers.onThinking) handlers.onThinking(data.status, data.message);
    });

    socket.on('tools-info', (data) => {
        // console.log('Received tools-info:', data);
        // Pass the data object directly, as the server emits the toolsByServer object itself
        if (handlers.onToolsInfo) handlers.onToolsInfo(data); 
    });

    socket.on('status-update', (data) => {
        // console.log('Received status-update:', data);
        if (handlers.onStatusUpdate) handlers.onStatusUpdate(data.message);
    });

    socket.on('error', (data) => {
        console.error('Received error event:', data.message);
        if (handlers.onError) handlers.onError(data.message);
        // Show important errors as toasts
        showToast('error', 'Server Error', data.message);
    });

    socket.on('conversation-cleared', () => {
        // console.log('Received conversation-cleared');
        if (handlers.onConversationCleared) handlers.onConversationCleared();
        // Toast handled by the action initiator in chatUI.js
    });

    socket.on('model-changed', (data) => {
        // console.log('Received model-changed:', data);
        if (handlers.onModelChanged) handlers.onModelChanged(data);
        // Toast/UI updates handled by the callback passed in init
    });

    socket.on('conversations-list', (data) => {
        // console.log('Received conversations-list:', data);
        if (handlers.onConversationsList) handlers.onConversationsList(data.conversations);
    });

    socket.on('conversation-loaded', (data) => {
        // console.log('Received conversation-loaded:', data);
        if (handlers.onConversationLoaded) handlers.onConversationLoaded(data);
    });

    socket.on('conversation-saved', (data) => {
        // console.log('Received conversation-saved:', data);
        if (handlers.onConversationSaved) handlers.onConversationSaved(data);
        // Toast handled by the callback
    });
}

// --- Emit Functions ---

export function emitUserMessage(message) {
    if (!socket) {
        console.error("Socket not initialized. Cannot send message.");
        showToast('error', 'Error', 'Not connected to server.');
        return;
    }
    socket.emit('user-message', { message });
}

export function emitClearConversation() {
     if (!socket) return;
     socket.emit('clear-conversation');
}

export function emitNewConversation() {
     if (!socket) return;
     socket.emit('new-conversation');
}

export function emitLoadConversation(conversationId) {
     if (!socket) return;
     socket.emit('load-conversation', { id: conversationId });
}

// Add other emit functions as needed
