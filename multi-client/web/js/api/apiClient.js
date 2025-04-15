// Handles fetch requests to the backend API

import { showToast } from '../ui/toast.js'; // Import toast for error handling

const API_BASE = '/api'; // Define base path for API routes

/**
 * Helper function for making fetch requests and handling common errors.
 * @param {string} endpoint The API endpoint (e.g., '/conversations').
 * @param {RequestInit} options Fetch options (method, headers, body, etc.).
 * @returns {Promise<any>} The JSON response data.
 * @throws {Error} If the request fails or returns an error status.
 */
async function fetchApi(endpoint, options = {}) {
    try {
        const response = await fetch(`${API_BASE}${endpoint}`, options);

        if (!response.ok) {
            let errorData;
            try {
                // Try to parse error response from backend
                errorData = await response.json();
            } catch (e) {
                // If parsing fails, use status text
                errorData = { error: response.statusText || `HTTP error ${response.status}` };
            }
            // Throw an error with the message from the backend or status text
            throw new Error(errorData.error || `Request failed with status ${response.status}`);
        }

        // If response is OK, try to parse JSON body
        // Handle cases with no content (e.g., DELETE requests)
        const contentType = response.headers.get("content-type");
        if (contentType && contentType.indexOf("application/json") !== -1) {
            return await response.json();
        } else {
            // Return null or an empty object if no JSON body
            return null;
        }
    } catch (error) {
        console.error(`API call to ${endpoint} failed:`, error);
        // Re-throw the error so the caller can handle it (e.g., show a toast)
        throw error;
    }
}

// --- Conversation API Calls ---

export async function fetchConversationsList() {
    try {
        const data = await fetchApi('/conversations');
        return data.conversations || [];
    } catch (error) {
        showToast('error', 'API Error', `Failed to load conversations: ${error.message}`);
        return []; // Return empty array on error
    }
}

export async function renameConversation(conversationId, newTitle) {
    try {
        await fetchApi(`/conversations/${conversationId}/rename`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ title: newTitle })
        });
        showToast('success', 'Success', 'Conversation renamed');
        return true;
    } catch (error) {
        showToast('error', 'API Error', `Failed to rename conversation: ${error.message}`);
        return false;
    }
}

export async function deleteConversation(conversationId) {
     try {
        await fetchApi(`/conversations/${conversationId}`, { method: 'DELETE' });
        showToast('success', 'Success', 'Conversation deleted');
        return true;
    } catch (error) {
        showToast('error', 'API Error', `Failed to delete conversation: ${error.message}`);
        return false;
    }
}

// --- AI/Model API Calls ---

export async function fetchProviders() {
     try {
        return await fetchApi('/ai/providers');
    } catch (error) {
        showToast('error', 'API Error', `Failed to load AI providers: ${error.message}`);
        // Return a default structure or rethrow based on how critical this is
        return { current: '', providers: {}, models: {} };
    }
}

export async function switchProviderAndModel(provider, model) {
    try {
        const data = await fetchApi('/ai/model', { // Use the combined endpoint
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ provider, model })
        });
        // Success toast/UI update is handled by the socket event listener
        return data; // Return { provider, model }
    } catch (error) {
        showToast('error', 'API Error', `Failed to switch provider/model: ${error.message}`);
        throw error; // Rethrow for caller if needed
    }
}

export async function updateApiKey(provider, apiKey) {
     try {
        const data = await fetchApi('/ai/keys', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ provider, apiKey })
        });
        // Success message might depend on whether the client was reloaded
        showToast('success', 'Success', data.message || `API key updated for ${provider}.`);
        return data; // Return response which might include warning/model info
    } catch (error) {
        showToast('error', 'API Error', `Failed to update API key: ${error.message}`);
        throw error;
    }
}

// --- Server Config API Calls ---

export async function fetchServerConfig() {
    try {
        // This fetches the servers.json content for editing
        return await fetchApi('/servers/config');
    } catch (error) {
        showToast('error', 'API Error', `Failed to load server configuration: ${error.message}`);
        return { mcpServers: {} }; // Return default structure on error
    }
}

export async function saveServerConfigurations(config) {
    try {
        const data = await fetchApi('/servers/config', { // Use the config endpoint
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ config }) // Send the full config object
        });
        showToast('success', 'Success', data.message || 'Server configuration saved.');
        // Remind user about restart if needed
        if (data.message && data.message.includes('Restart')) {
             showToast('warning', 'Restart Required', 'Restart the application to apply server changes.');
        }
        return true;
    } catch (error) {
        showToast('error', 'API Error', `Failed to save server configuration: ${error.message}`);
        return false;
    }
}

// --- General Config File API Calls ---

export async function fetchConfigFile(fileName) {
    try {
        return await fetchApi(`/config/${fileName}`);
    } catch (error) {
        showToast('error', 'API Error', `Failed to load ${fileName}: ${error.message}`);
        throw error; // Rethrow for modal handling
    }
}

export async function saveConfigFile(fileName, content) {
    try {
        const data = await fetchApi(`/config/${fileName}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ content })
        });
        showToast('success', 'Success', data.message || `${fileName} saved successfully.`);
         if (data.needsRestart) {
             showToast('warning', 'Restart Required', 'Restart the application for changes to take effect.');
         }
        return data; // Return { success, message, needsRestart }
    } catch (error) {
        showToast('error', 'API Error', `Failed to save ${fileName}: ${error.message}`);
        throw error; // Rethrow for modal handling
    }
}
