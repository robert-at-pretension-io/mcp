// @ts-nocheck
import express from 'express';
import http from 'http';
import cors from 'cors';
import { Server as SocketIOServer } from 'socket.io';
import path from 'path';
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
    this.app.use(express.static(path.join(__dirname, '../../web')));

    // Create HTTP server
    this.server = http.createServer(this.app);
    
    // Set up Socket.IO
    this.io = new SocketIOServer(this.server, {
      cors: {
        origin: '*',
        methods: ['GET', 'POST']
      }
    });

    // Set up routes and socket events
    this.setupRoutes();
    this.setupSocketEvents();
  }

  private setupRoutes() {
    // Root route - serve the main HTML file
    this.app.get('/', (req, res) => {
      res.sendFile(path.join(__dirname, '../../web/index.html'));
    });

    // API route for server information
    this.app.get('/api/servers', (req, res) => {
      const connectedServers = this.serverManager.getConnectedServers();
      res.json({ servers: connectedServers });
    });

    // API route for conversation history
    this.app.get('/api/history', (req, res) => {
      const history = this.conversationManager.getHistory();
      res.json({ 
        history: history.map(msg => ({
          role: msg._getType(),
          content: msg.content,
          hasToolCalls: msg.hasToolCalls,
          pendingToolCalls: msg.pendingToolCalls
        })) 
      });
    });

    // API route for available tools
    this.app.get('/api/tools', async (req, res) => {
      try {
        const tools = await this.serverManager.getAllTools();
        res.json({ tools });
      } catch (error) {
        res.status(500).json({ error: `Failed to get tools: ${error}` });
      }
    });
    
    // API route for AI model info
    this.app.get('/api/model', (req, res) => {
      try {
        const model = this.conversationManager.getAiClientModelName();
        res.json({ model });
      } catch (error) {
        res.status(500).json({ error: `Failed to get AI model info: ${error}` });
      }
    });

    // API route for submitting a message
    this.app.post('/api/message', (req, res) => {
      try {
        const { message } = req.body;
        if (!message) {
          return res.status(400).json({ error: 'Message is required' });
        }

        // Process the message in a non-blocking way
        this.processUserMessage(message);
        
        // Return immediately to not block the request
        res.json({ status: 'processing' });
      } catch (error) {
        res.status(500).json({ error: `Failed to process message: ${error}` });
      }
    });

    // API route for clearing the conversation
    this.app.post('/api/clear', (req, res) => {
      try {
        this.conversationManager.clearConversation();
        res.json({ status: 'success' });
        this.io.emit('conversation-cleared');
      } catch (error) {
        res.status(500).json({ error: `Failed to clear conversation: ${error}` });
      }
    });
  }

  private setupSocketEvents() {
    this.io.on('connection', (socket) => {
      console.log('Client connected:', socket.id);

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
        this.conversationManager.clearConversation();
        this.io.emit('conversation-cleared');
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

      // Send available tools
      const tools = await this.serverManager.getAllTools();
      socket.emit('tools-info', { tools });
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
      
      // Emit the AI response
      this.io.emit('ai-response', { 
        role: 'ai',
        content: aiResponse
      });
      
      // Send updated history
      const history = this.conversationManager.getHistory();
      this.io.emit('history-update', { 
        history: history.map(msg => ({
          role: msg._getType(),
          content: msg.content,
          hasToolCalls: msg.hasToolCalls,
          pendingToolCalls: msg.pendingToolCalls
        }))
      });
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
    return new Promise((resolve, reject) => {
      if (!this.isRunning) {
        resolve();
        return;
      }
      
      this.server.close((err) => {
        if (err) {
          reject(err);
        } else {
          console.log('Web server stopped');
          this.isRunning = false;
          resolve();
        }
      });
    });
  }
}