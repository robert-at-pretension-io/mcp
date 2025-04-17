// @ts-nocheck
import express from 'express';
import http from 'http';
import cors from 'cors';
import { Server as SocketIOServer } from 'socket.io';
import path from 'path';
import fs from 'fs';
import { fileURLToPath } from 'url';
import type { ConversationManager } from '../conversation/ConversationManager.js';
import type { ServerManager } from '../ServerManager.js';

// Helper for ES modules to get the directory path
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export class WebServer {
  private app;
  private server;
  private io;
  private conversationManager;
  private serverManager;
  private port;
  private isRunning = false;

  constructor(conversationManager, serverManager, port = 3000) {
    this.conversationManager = conversationManager;
    this.serverManager = serverManager;
    this.port = port;

    // Initialize Express app
    this.app = express();
    this.app.use(cors());
    this.app.use(express.json());
    
    // Serve static files from the web directory
    // Correct path: Go up three levels from dist/src/web to multi-client/, then into web/
    const webDirPath = path.join(__dirname, '../../../web'); 
    this.app.use(express.static(webDirPath));

    // Create HTTP server
    this.server = http.createServer(this.app);
    
    // Set up Socket.IO
    this.io = new SocketIOServer(this.server, {
      cors: {
        origin: '*',
        methods: ['GET', 'POST']
      }
    });
  }

  /**
   * Asynchronously initializes routes and socket events after dynamic imports.
   */
  async init() {
    // Import routers within the async method
    const { createAiRouter } = await import('./routes/ai.routes.js');
    const { createServersRouter } = await import('./routes/servers.routes.js');
    const { createConfigRouter } = await import('./routes/config.routes.js');
    const { createConversationRouter } = await import('./routes/conversation.routes.js');

    // Set up routes and socket events
    this.setupRoutes(
      createAiRouter(this.conversationManager, this.serverManager, this.io), // Pass io instance here
      createServersRouter(this.serverManager),
      createConfigRouter(),
      createConversationRouter(this.conversationManager, this.io) // Pass io for emitting events
    );
    this.setupSocketEvents();
  }

  private setupRoutes(aiRouter, serversRouter, configRouter, conversationRouter) {
    // Correct path: Go up three levels from dist/src/web to multi-client/, then into web/
    const webDirPath = path.join(__dirname, '../../../web');

    // Root route - serve the main HTML file
    this.app.get('/', (req, res) => {
      res.sendFile(path.join(webDirPath, 'index.html'));
    });

    // Mount API routers under /api prefix
    this.app.use('/api/ai', aiRouter);
    this.app.use('/api/servers', serversRouter);
    this.app.use('/api/config', configRouter);
    this.app.use('/api/conversations', conversationRouter);

    // --- Deprecated/Moved Routes (Remove or keep for backward compatibility if needed) ---
    // These are now handled by the specific routers

    // Example: Remove old /api/servers route if handled by serversRouter
    // this.app.get('/api/servers', (req, res) => { ... });

    // All old route definitions previously here should be removed as they are now
    // handled by the imported routers (aiRouter, serversRouter, configRouter, conversationRouter).
  }

  private setupSocketEvents() {
    this.io.on('connection', (socket) => {
      console.log('Client connected:', socket.id);

      // General error handler for this specific socket
      socket.on('error', (err) => {
        console.error(`[WebServer] Socket Error for ${socket.id}:`, err);
        // Depending on the error, you might want to disconnect the client
        // or just log it.
      });

      // Send initial data to newly connected client
      this.sendInitialData(socket);

      // Handle disconnect
      socket.on('disconnect', () => {
        console.log('Client disconnected:', socket.id);
      });

      // Handle user messages from socket
      socket.on('user-message', (data) => {
        const { message } = data;
        this.processUserMessage(message);
      });

      // Handle clear conversation request
      socket.on('clear-conversation', () => {
        try {
          this.conversationManager.clearConversation();
          this.io.emit('conversation-cleared'); // Notify all clients
        } catch (error) {
          console.error('[WebServer] Error clearing conversation:', error);
          socket.emit('error', { // Notify specific client about the error
            message: `Error clearing conversation: ${error instanceof Error ? error.message : String(error)}`
          });
        }
      });

      // Handle new conversation request
      socket.on('new-conversation', () => {
        try {
          this.conversationManager.newConversation();
          const history = this.conversationManager.getHistory();

          // Emit the new conversation ID
          const currentConversation = this.conversationManager.getCurrentConversation();
          this.io.emit('conversation-loaded', { // Notify all clients about the new state
            id: currentConversation.id,
            messages: history.map(msg => ({
              role: msg._getType(),
              content: msg.content,
              hasToolCalls: msg.hasToolCalls,
              pendingToolCalls: msg.pendingToolCalls
            }))
          });
          
          // Send the updated conversations list
          const conversations = this.conversationManager.listConversations();
          this.io.emit('conversations-list', { conversations });
        } catch (error) {
          console.error('[WebServer] Error creating new conversation:', error);
          socket.emit('error', { // Notify specific client about the error
            message: `Error creating new conversation: ${error instanceof Error ? error.message : String(error)}`
          });
        }
      });

      // Handle load conversation request
      socket.on('load-conversation', (data) => {
        try {
          const { id } = data;
          if (!id) {
            throw new Error('Conversation ID is required');
          }

          const success = this.conversationManager.loadConversation(id);
          if (!success) {
            throw new Error(`Conversation with ID ${id} not found`);
          }
          
          const history = this.conversationManager.getHistory();
          
          // Emit the loaded conversation
          this.io.emit('conversation-loaded', {
            id,
            messages: history.map(msg => ({
              role: msg._getType(),
              content: msg.content,
              hasToolCalls: msg.hasToolCalls,
              pendingToolCalls: msg.pendingToolCalls
            }))
          });
        } catch (error) {
          console.error('[WebServer] Error loading conversation:', error);
          socket.emit('error', { // Notify specific client about the error
            message: `Error loading conversation: ${error instanceof Error ? error.message : String(error)}`
          });
        }
      });
    });
  }

  private async sendInitialData(socket) {
    try {
      // Send server information
      const connectedServers = this.serverManager.getConnectedServers();
      socket.emit('servers-info', { servers: connectedServers });

      // Send conversation history
      const history = this.conversationManager.getHistory();
      socket.emit('history', { 
        history: history.map(msg => ({
          role: msg._getType(),
          content: msg.content,
          hasToolCalls: msg.hasToolCalls,
          pendingToolCalls: msg.pendingToolCalls
        }))
      });

      // Send available tools, organizing by server
      const allTools = await this.serverManager.getAllTools();
      const serverNames = this.serverManager.getConnectedServers();

      // Format tools as { serverName: toolsArray }
      const toolsByServer: Record<string, Tool[]> = {}; // Ensure correct type and declare only once
      for (const serverName of serverNames) {
        try {
          // Attempt to get tools, but handle potential errors gracefully
          const serverTools = this.serverManager.getServerTools(serverName);
          toolsByServer[serverName] = serverTools;
        } catch (error) {
          // Log the error but continue fetching from other servers
          console.warn(`[WebServer] Error getting tools for server ${serverName} during initial data send:`, error instanceof Error ? error.message : String(error));
          toolsByServer[serverName] = []; // Assign empty array for this server on error
        }
      }

      console.log('[WebServer] Emitting tools-info:', JSON.stringify(Object.keys(toolsByServer))); // Log server names with tools
      socket.emit('tools-info', toolsByServer);
      
      // Send the list of conversations
      try {
        const conversations = this.conversationManager.listConversations();
        socket.emit('conversations-list', { conversations });
        
        // If there's a current conversation, send its ID
        const currentConversation = this.conversationManager.getCurrentConversation();
        if (currentConversation && currentConversation.id) {
          socket.emit('conversation-loaded', { 
            id: currentConversation.id,
            messages: history.map(msg => ({
              role: msg._getType(),
              content: msg.content,
              hasToolCalls: msg.hasToolCalls,
              pendingToolCalls: msg.pendingToolCalls
            }))
          });
        }
      } catch (conversationError) {
        console.error('Error sending conversations list:', conversationError);
        // Continue even if this part fails
      }
    } catch (error) {
      console.error('Error sending initial data:', error);
    }
  }

  private async processUserMessage(message) {
    try {
      // Emit 'thinking' event to indicate processing has started
      this.io.emit('thinking', { status: true });
      
      // Process the message
      const aiResponse = await this.conversationManager.processUserMessage(message);
      
      // Emit 'thinking' event to indicate processing has finished
      this.io.emit('thinking', { status: false });
      
      // Send updated history (which includes the new AI response)
      const history = this.conversationManager.getHistory();
      this.io.emit('history-update', { 
        history: history.map(msg => ({
          role: msg._getType(),
          content: msg.content,
          hasToolCalls: msg.hasToolCalls,
          pendingToolCalls: msg.pendingToolCalls
        }))
      });
      
      // If we have a current conversation, emit the updated conversation
      try {
        const currentConversation = this.conversationManager.getCurrentConversation();
        if (currentConversation && currentConversation.id) {
          this.io.emit('conversation-saved', currentConversation);
        }
      } catch (conversationError) {
        console.error('Error getting current conversation:', conversationError);
        // Continue even if this part fails
      }
    } catch (error) {
      console.error('Error processing message:', error);
      this.io.emit('thinking', { status: false });
      this.io.emit('error', { 
        message: `Error processing message: ${error instanceof Error ? error.message : String(error)}`
      });
    }
  }

  /**
   * Start the web server
   */
  public start() {
    if (this.isRunning) return;
    
    this.server.listen(this.port, () => {
      console.log(`Web server running at http://localhost:${this.port}`);
      this.isRunning = true;
    });
  }

  /**
   * Stop the web server
   */
  public stop() {
    return new Promise<void>((resolve, reject) => {
      if (!this.isRunning) {
        resolve();
        return;
      }
      
      // First close all socket connections
      if (this.io) {
        console.log('Closing Socket.IO connections');
        this.io.close(() => {
          // After socket connections closed, close the HTTP server
          this.server.close((err) => {
            if (err) {
              console.error('Error closing HTTP server:', err);
              reject(err);
            } else {
              console.log('Web server stopped');
              this.isRunning = false;
              resolve();
            }
          });
        });
      } else {
        // If io doesn't exist, just close the HTTP server
        this.server.close((err) => {
          if (err) {
            console.error('Error closing HTTP server:', err);
            reject(err);
          } else {
            console.log('Web server stopped');
            this.isRunning = false;
            resolve();
          }
        });
      }
      
      // Set a timeout in case the server doesn't close properly
      const timeout = setTimeout(() => {
        console.log('Server stop timed out, forcing close');
        this.isRunning = false;
        resolve();
      }, 3000); // 3 seconds timeout
      
      // Clear the timeout if the server closes properly
      timeout.unref();
    });
  }
}
