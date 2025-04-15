import { Router, type Request, type Response, type RequestHandler } from 'express';
import type { ConversationManager } from '../../conversation/ConversationManager.js';
import type { Server as SocketIOServer } from 'socket.io'; // Import Socket.IO Server type

export function createConversationRouter(
    conversationManager: ConversationManager,
    io: SocketIOServer // Pass the Socket.IO server instance
): Router {
    const router = Router();

    // --- Get Conversation History (Potentially redundant if using sockets) ---
    // Kept for potential direct API access if needed
    router.get('/history', ((req, res) => {
        try {
            const history = conversationManager.getHistory();
            res.json({
                history: history.map(msg => ({
                    role: msg._getType(),
                    content: msg.content,
                    // Ensure these properties exist or handle potential undefined
                    hasToolCalls: (msg as any).hasToolCalls ?? false,
                    pendingToolCalls: (msg as any).pendingToolCalls ?? false
                }))
            });
        } catch (error) {
            res.status(500).json({ error: `Failed to get history: ${error instanceof Error ? error.message : String(error)}` });
        }
    }) as RequestHandler);

    // --- List Conversations ---
    router.get('/', ((req, res) => {
        try {
            const conversations = conversationManager.listConversations();
            res.json({ conversations });
        } catch (error) {
            res.status(500).json({ error: `Failed to list conversations: ${error instanceof Error ? error.message : String(error)}` });
        }
    }) as RequestHandler);

    // --- Create New Conversation ---
    // This might be better handled purely via sockets, but providing an API endpoint too
    router.post('/new', ((req, res) => {
        try {
            conversationManager.newConversation();
            const currentConversation = conversationManager.getCurrentConversation(); // Get metadata
            const history = conversationManager.getHistory(); // Get messages for the new empty convo

            // Emit socket events (handled in WebServer or here if io is passed)
            io.emit('conversation-loaded', {
                id: currentConversation.id,
                messages: history.map(msg => ({
                    role: msg._getType(),
                    content: msg.content,
                    hasToolCalls: (msg as any).hasToolCalls ?? false,
                    pendingToolCalls: (msg as any).pendingToolCalls ?? false
                }))
            });
            const conversations = conversationManager.listConversations();
            io.emit('conversations-list', { conversations });

            res.json({ success: true, id: currentConversation.id });
        } catch (error) {
            res.status(500).json({ error: `Failed to create new conversation: ${error instanceof Error ? error.message : String(error)}` });
        }
    }) as RequestHandler);

    // --- Load Conversation ---
    // Also potentially better via sockets
    router.post('/load', ((req, res) => {
        try {
            const { id } = req.body;
            if (!id) {
                return res.status(400).json({ error: 'Conversation ID is required' });
            }

            const success = conversationManager.loadConversation(id);
            if (!success) {
                return res.status(404).json({ error: `Conversation with ID ${id} not found` });
            }

            const history = conversationManager.getHistory();

            // Emit socket event
            io.emit('conversation-loaded', {
                id,
                messages: history.map(msg => ({
                    role: msg._getType(),
                    content: msg.content,
                    hasToolCalls: (msg as any).hasToolCalls ?? false,
                    pendingToolCalls: (msg as any).pendingToolCalls ?? false
                }))
            });

            res.json({ success: true, id });
        } catch (error) {
            res.status(500).json({ error: `Failed to load conversation: ${error instanceof Error ? error.message : String(error)}` });
        }
    }) as RequestHandler);

    // --- Delete Conversation ---
    router.delete('/:id', ((req, res) => {
        try {
            const { id } = req.params;
            if (!id) {
                return res.status(400).json({ error: 'Conversation ID is required' });
            }

            const success = conversationManager.deleteConversation(id);
            if (!success) {
                return res.status(404).json({ error: `Conversation with ID ${id} not found` });
            }

            // Emit socket event
            const conversations = conversationManager.listConversations();
            io.emit('conversations-list', { conversations });

            res.json({ success: true });
        } catch (error) {
            res.status(500).json({ error: `Failed to delete conversation: ${error instanceof Error ? error.message : String(error)}` });
        }
    }) as RequestHandler);

    // --- Rename Conversation ---
    router.post('/:id/rename', ((req, res) => {
        try {
            const { id } = req.params;
            const { title } = req.body;

            if (!id) {
                return res.status(400).json({ error: 'Conversation ID is required' });
            }
            if (!title) {
                return res.status(400).json({ error: 'Title is required' });
            }

            const success = conversationManager.renameConversation(id, title);
            if (!success) {
                return res.status(404).json({ error: `Conversation with ID ${id} not found` });
            }

            // Emit socket event
            const conversations = conversationManager.listConversations();
            io.emit('conversations-list', { conversations });

            res.json({ success: true });
        } catch (error) {
            res.status(500).json({ error: `Failed to rename conversation: ${error instanceof Error ? error.message : String(error)}` });
        }
    }) as RequestHandler);

    // --- Submit Message (Potentially redundant if using sockets) ---
    // Kept for potential direct API access
    router.post('/message', (async (req, res) => {
        try {
            const { message } = req.body;
            if (!message) {
                return res.status(400).json({ error: 'Message is required' });
            }

            // Process the message (this is async)
            // We don't wait for the full response here, just acknowledge receipt
            conversationManager.processUserMessage(message)
                .then(aiResponse => {
                    // Emit events via socket (handled in ConversationManager or WebServer)
                    // Example: io.emit('history-update', ...);
                    console.log("AI response processed (via API route)");
                })
                .catch(error => {
                    console.error("Error processing message via API route:", error);
                    // Emit error via socket
                    io.emit('error', { message: `Error processing message: ${error.message}` });
                });

            // Return immediately
            res.json({ status: 'processing' });
        } catch (error) {
            res.status(500).json({ error: `Failed to process message: ${error instanceof Error ? error.message : String(error)}` });
        }
    }) as RequestHandler);

    // --- Clear Conversation (Potentially redundant if using sockets) ---
    router.post('/clear', ((req, res) => {
        try {
            conversationManager.clearConversation();
            // Emit socket event
            io.emit('conversation-cleared');
            res.json({ status: 'success' });
        } catch (error) {
            res.status(500).json({ error: `Failed to clear conversation: ${error instanceof Error ? error.message : String(error)}` });
        }
    }) as RequestHandler);


    return router;
}
