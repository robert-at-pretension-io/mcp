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

    // Set up routes and socket events
    this.setupRoutes();
    this.setupSocketEvents();
  }

  private setupRoutes() {
    // Correct path: Go up three levels from dist/src/web to multi-client/, then into web/
    const webDirPath = path.join(__dirname, '../../../web'); 
    
    // Root route - serve the main HTML file
    this.app.get('/', (req, res) => {
      res.sendFile(path.join(webDirPath, 'index.html'));
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
        // Format tools as { serverName: toolsArray }
        const connectedServers = this.serverManager.getConnectedServers();
        const toolsByServer = {};
        
        for (const serverName of connectedServers) {
          try {
            const serverTools = this.serverManager.getServerTools(serverName);
            toolsByServer[serverName] = serverTools;
          } catch (error) {
            console.warn(`Error getting tools for server ${serverName}:`, error);
            toolsByServer[serverName] = [];
          }
        }
        
        res.json(toolsByServer);
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
    
    // API routes for conversation management
    this.app.get('/api/conversations', (req, res) => {
      try {
        const conversations = this.conversationManager.listConversations();
        res.json({ conversations });
      } catch (error) {
        res.status(500).json({ error: `Failed to list conversations: ${error}` });
      }
    });
    
    this.app.post('/api/conversations/new', (req, res) => {
      try {
        this.conversationManager.newConversation();
        
        // Get the new conversation
        const currentConversation = this.conversationManager.getCurrentConversation();
        
        // Send updated history
        const history = this.conversationManager.getHistory();
        this.io.emit('history-update', { 
          history: history.map(msg => ({
            role: msg._getType(),
            content: msg.content,
            hasToolCalls: (msg as any).hasToolCalls,
            pendingToolCalls: (msg as any).pendingToolCalls
          }))
        });
        
        // Emit the conversation-loaded event
        this.io.emit('conversation-loaded', {
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
        
        res.json({ success: true, id: currentConversation.id });
      } catch (error) {
        res.status(500).json({ error: `Failed to create new conversation: ${error}` });
      }
    });
    
    this.app.post('/api/conversations/load', (req, res) => {
      try {
        const { id } = req.body;
        
        if (!id) {
          return res.status(400).json({ error: 'Conversation ID is required' });
        }
        
        const success = this.conversationManager.loadConversation(id);
        
        if (!success) {
          return res.status(404).json({ error: `Conversation with ID ${id} not found` });
        }
        
        // Send updated history
        const history = this.conversationManager.getHistory();
        
        // Emit the conversation-loaded event
        this.io.emit('conversation-loaded', {
          id,
          messages: history.map(msg => ({
            role: msg._getType(),
            content: msg.content,
            hasToolCalls: msg.hasToolCalls,
            pendingToolCalls: msg.pendingToolCalls
          }))
        });
        
        res.json({ success: true, id });
      } catch (error) {
        res.status(500).json({ error: `Failed to load conversation: ${error}` });
      }
    });
    
    // API route for deleting a conversation
    this.app.delete('/api/conversations/:id', (req, res) => {
      try {
        const { id } = req.params;
        
        if (!id) {
          return res.status(400).json({ error: 'Conversation ID is required' });
        }
        
        const success = this.conversationManager.deleteConversation(id);
        
        if (!success) {
          return res.status(404).json({ error: `Conversation with ID ${id} not found` });
        }
        
        // Send the updated conversations list
        const conversations = this.conversationManager.listConversations();
        this.io.emit('conversations-list', { conversations });
        
        res.json({ success: true });
      } catch (error) {
        res.status(500).json({ error: `Failed to delete conversation: ${error}` });
      }
    });
    
    // API route for renaming a conversation
    this.app.post('/api/conversations/:id/rename', (req, res) => {
      try {
        const { id } = req.params;
        const { title } = req.body;
        
        if (!id) {
          return res.status(400).json({ error: 'Conversation ID is required' });
        }
        
        if (!title) {
          return res.status(400).json({ error: 'Title is required' });
        }
        
        const success = this.conversationManager.renameConversation(id, title);
        
        if (!success) {
          return res.status(404).json({ error: `Conversation with ID ${id} not found` });
        }
        
        // Send the updated conversations list
        const conversations = this.conversationManager.listConversations();
        this.io.emit('conversations-list', { conversations });
        
        res.json({ success: true });
      } catch (error) {
        res.status(500).json({ error: `Failed to rename conversation: ${error}` });
      }
    });
    
    // API route for managing server configurations
    this.app.get('/api/servers', async (req, res) => {
      try {
        // Load servers.json file
        const configPath = path.join(__dirname, '../../../servers.json');
        const serversConfigFile = await fs.promises.readFile(configPath, 'utf-8');
        const serversConfig = JSON.parse(serversConfigFile);
        
        res.json(serversConfig);
      } catch (error) {
        res.status(500).json({ error: `Failed to get server configurations: ${error}` });
      }
    });
    
    // API route for updating server configurations
    this.app.post('/api/servers', async (req, res) => {
      try {
        const { config } = req.body;
        
        if (!config || typeof config !== 'object' || !config.mcpServers) {
          return res.status(400).json({ error: 'Invalid server configuration format. Must include mcpServers object.' });
        }
        
        // Validate basic structure
        if (typeof config.mcpServers !== 'object') {
          return res.status(400).json({ error: 'mcpServers must be an object.' });
        }
        
        // Validate each server entry
        for (const [serverName, serverConfig] of Object.entries(config.mcpServers)) {
          if (!serverConfig || typeof serverConfig !== 'object') {
            return res.status(400).json({ error: `Server configuration for "${serverName}" is invalid.` });
          }
          
          if (!serverConfig.command || typeof serverConfig.command !== 'string') {
            return res.status(400).json({ error: `Server "${serverName}" must have a command property.` });
          }
          
          if (serverConfig.args && !Array.isArray(serverConfig.args)) {
            return res.status(400).json({ error: `Server "${serverName}" args must be an array.` });
          }
          
          if (serverConfig.env && typeof serverConfig.env !== 'object') {
            return res.status(400).json({ error: `Server "${serverName}" env must be an object.` });
          }
        }
        
        // Save the configuration
        const configPath = path.join(__dirname, '../../../servers.json');
        await fs.promises.writeFile(configPath, JSON.stringify(config, null, 2), 'utf-8');
        
        // Signal that server needs to be restarted
        res.json({ 
          success: true, 
          message: 'Server configuration updated. Restart the application to apply changes.' 
        });
      } catch (error) {
        res.status(500).json({ error: `Failed to update server configurations: ${error}` });
      }
    });
    
    // API route for updating API keys
    this.app.post('/api/keys', async (req, res) => {
      try {
        const { provider, apiKey } = req.body;
        
        if (!provider || typeof provider !== 'string') {
          return res.status(400).json({ error: 'Provider name is required' });
        }
        
        if (!apiKey || typeof apiKey !== 'string') {
          return res.status(400).json({ error: 'API key is required' });
        }
        
        // Load AI config
        const configPath = path.join(__dirname, '../../../ai_config.json');
        const aiConfigFile = await fs.promises.readFile(configPath, 'utf-8');
        const aiConfigData = JSON.parse(aiConfigFile);
        
        // Check if provider exists
        if (!aiConfigData.providers[provider]) {
          return res.status(404).json({ error: `Provider "${provider}" not found` });
        }
        
        // Update the API key for the provider
        aiConfigData.providers[provider].apiKey = apiKey;
        
        // If there's an apiKeyEnvVar, remove it as we're now using a direct key
        if (aiConfigData.providers[provider].apiKeyEnvVar) {
          delete aiConfigData.providers[provider].apiKeyEnvVar;
        }
        
        // Save the updated config
        await fs.promises.writeFile(configPath, JSON.stringify(aiConfigData, null, 2), 'utf-8');
        
        // Try to update the environment variable for the current session
        const defaultProviderKey = provider.toLowerCase();
        const envVarMap = {
          'openai': 'OPENAI_API_KEY',
          'anthropic': 'ANTHROPIC_API_KEY',
          'google-genai': 'GOOGLE_API_KEY',
          'mistralai': 'MISTRAL_API_KEY',
          'fireworks': 'FIREWORKS_API_KEY'
        };
        
        const envVar = envVarMap[defaultProviderKey];
        if (envVar) {
          process.env[envVar] = apiKey;
        }
        
        // Reload the AI client if this is the current provider
        if (provider === aiConfigData.defaultProvider) {
          try {
            // Load provider models
            const providerModelsPath = path.join(__dirname, '../../../provider_models.toml');
            const providerModelsContent = await fs.promises.readFile(providerModelsPath, 'utf-8');
            
            // Parse TOML
            const TOML = (await import('@ltd/j-toml')).default;
            let providerModels = {};
            
            try {
              const parsedToml = TOML.parse(providerModelsContent, { joiner: '\n', bigint: false });
              
              if (typeof parsedToml === 'object' && parsedToml !== null) {
                providerModels = Object.entries(parsedToml).reduce((acc, [key, value]) => {
                  if (typeof value === 'object' && value !== null && Array.isArray((value as any).models)) {
                    acc[key.toLowerCase()] = { models: (value as any).models };
                  }
                  return acc;
                }, {});
              }
            } catch (tomlError) {
              console.error('Error parsing TOML:', tomlError);
              // Continue without models if parsing fails
            }
            
            // Switch the client with the new API key
            const providerConfig = aiConfigData.providers[provider];
            const model = this.conversationManager.switchAiClient(providerConfig, providerModels);
            
            // Emit model changed event (even though model may be the same, the API key changed)
            this.io.emit('model-changed', { provider, model });
            
            res.json({ 
              success: true, 
              message: 'API key updated and applied.',
              provider,
              model
            });
          } catch (clientError) {
            // If switching client fails, still return success for updating the config
            console.error('Error switching client after API key update:', clientError);
            res.json({ 
              success: true, 
              warning: 'API key was saved but could not be applied immediately. You may need to restart the application.',
              error: String(clientError)
            });
          }
        } else {
          res.json({ 
            success: true, 
            message: 'API key updated for provider: ' + provider
          });
        }
      } catch (error) {
        res.status(500).json({ error: `Failed to update API key: ${error}` });
      }
    });
    
    // API route for getting AI providers info
    this.app.get('/api/providers', async (req, res) => {
      try {
        // Load AI config file
        const configPath = path.join(__dirname, '../../../ai_config.json');
        const aiConfigFile = await fs.promises.readFile(configPath, 'utf-8');
        const aiConfigData = JSON.parse(aiConfigFile);
        
        // Load provider models file
        const providerModelsPath = path.join(__dirname, '../../../provider_models.toml');
        const providerModelsFile = await fs.promises.readFile(providerModelsPath, 'utf-8');
        
        // Import TOML dynamically (since we're already using it in the project)
        const TOML = (await import('@ltd/j-toml')).default;
        let providerModels = {};
        
        try {
          // Try to parse with the proper TOML parser first
          const parsedToml = TOML.parse(providerModelsFile, { joiner: '\n', bigint: false });
          
          if (typeof parsedToml === 'object' && parsedToml !== null) {
            providerModels = Object.entries(parsedToml).reduce((acc, [key, value]) => {
              if (typeof value === 'object' && value !== null && Array.isArray((value as any).models)) {
                acc[key.toLowerCase()] = { models: (value as any).models };
              }
              return acc;
            }, {});
          }
        } catch (tomlError) {
          console.error('Error parsing TOML with parser, falling back to regex:', tomlError);
          
          // Fallback to regex-based parsing if TOML parser fails
          providerModels = {};
          const providerSections = providerModelsFile.split(/\[\w+\]/g).filter(Boolean);
          const sectionNames = providerModelsFile.match(/\[(\w+)\]/g);
          
          if (sectionNames) {
            sectionNames.forEach((name, index) => {
              const providerName = name.replace(/\[|\]/g, '');
              const section = providerSections[index];
              const modelsMatch = section.match(/models\s*=\s*\[([\s\S]*?)\]/);
              
              if (modelsMatch) {
                const modelsText = modelsMatch[1];
                const models = modelsText.match(/"([^"]+)"/g)?.map(m => m.replace(/"/g, '')) || [];
                providerModels[providerName] = { models };
              }
            });
          }
        }
        res.json({ 
          current: aiConfigData.defaultProvider,
          providers: aiConfigData.providers,
          models: providerModels
        });
      } catch (error) {
        res.status(500).json({ error: `Failed to get AI providers info: ${error}` });
      }
    });
    
    // API route for switching the AI provider
    this.app.post('/api/provider', async (req, res) => {
      try {
        const { provider } = req.body;
        if (!provider) {
          return res.status(400).json({ error: 'Provider name is required' });
        }
        
        // Load AI config
        const configPath = path.join(__dirname, '../../../ai_config.json');
        const aiConfigFile = await fs.promises.readFile(configPath, 'utf-8');
        const aiConfigData = JSON.parse(aiConfigFile);
        
        // Check if provider exists
        if (!aiConfigData.providers[provider]) {
          return res.status(404).json({ error: `Provider "${provider}" not found` });
        }
        
        // Load provider models
        const providerModelsPath = path.join(__dirname, '../../../provider_models.toml');
        const providerModelsContent = await fs.promises.readFile(providerModelsPath, 'utf-8');
        
        // Import TOML dynamically
        const TOML = (await import('@ltd/j-toml')).default;
        let providerModels = {};
        
        try {
          // Try to parse with the proper TOML parser first
          const parsedToml = TOML.parse(providerModelsContent, { joiner: '\n', bigint: false });
          
          if (typeof parsedToml === 'object' && parsedToml !== null) {
            providerModels = Object.entries(parsedToml).reduce((acc, [key, value]) => {
              if (typeof value === 'object' && value !== null && Array.isArray((value as any).models)) {
                acc[key.toLowerCase()] = { models: (value as any).models };
              }
              return acc;
            }, {});
          }
        } catch (tomlError) {
          console.error('Error parsing TOML with parser, falling back to regex:', tomlError);
          
          // Fallback to regex-based parsing if TOML parser fails
          providerModels = {};
          const providerSections = providerModelsContent.split(/\[\w+\]/g).filter(Boolean);
          const sectionNames = providerModelsContent.match(/\[(\w+)\]/g);
          
          if (sectionNames) {
            sectionNames.forEach((name, index) => {
              const providerName = name.replace(/\[|\]/g, '');
              const section = providerSections[index];
              const modelsMatch = section.match(/models\s*=\s*\[([\s\S]*?)\]/);
              
              if (modelsMatch) {
                const modelsText = modelsMatch[1];
                const models = modelsText.match(/"([^"]+)"/g)?.map(m => m.replace(/"/g, '')) || [];
                providerModels[providerName] = { models };
              }
            });
          }
        }
        
        // Update default provider
        aiConfigData.defaultProvider = provider;
        
        // Save the updated config
        await fs.promises.writeFile(configPath, JSON.stringify(aiConfigData, null, 2), 'utf-8');
        
        // Switch the provider
        const providerConfig = aiConfigData.providers[provider];
        const model = this.conversationManager.switchAiClient(providerConfig, providerModels);
        
        // Send updated history after switching since it clears conversation
        const history = this.conversationManager.getHistory();
        this.io.emit('history-update', { 
          history: history.map(msg => ({
            role: msg._getType(),
            content: msg.content,
            hasToolCalls: msg.hasToolCalls,
            pendingToolCalls: msg.pendingToolCalls
          }))
        });
        
        // Emit model changed event
        this.io.emit('model-changed', { provider, model });
        
        res.json({ provider, model });
      } catch (error) {
        res.status(500).json({ error: `Failed to switch provider: ${error}` });
      }
    });
    
    // API route for switching the model
    this.app.post('/api/model', async (req, res) => {
      try {
        const { model, provider } = req.body;
        if (!model) {
          return res.status(400).json({ error: 'Model name is required' });
        }
        
        // Load AI config
        const configPath = path.join(__dirname, '../../../ai_config.json');
        const aiConfigFile = await fs.promises.readFile(configPath, 'utf-8');
        const aiConfigData = JSON.parse(aiConfigFile);
        
        // Determine which provider to use
        const providerName = provider || aiConfigData.defaultProvider;
        if (!providerName || !aiConfigData.providers[providerName]) {
          return res.status(404).json({ error: `Provider "${providerName}" not found` });
        }
        
        // Update the provider's model
        aiConfigData.providers[providerName].model = model;
        
        // Save the updated config
        await fs.promises.writeFile(configPath, JSON.stringify(aiConfigData, null, 2), 'utf-8');
        
        // Load provider models for validation
        const providerModelsPath = path.join(__dirname, '../../../provider_models.toml');
        const providerModelsContent = await fs.promises.readFile(providerModelsPath, 'utf-8');
        
        // Import TOML dynamically
        const TOML = (await import('@ltd/j-toml')).default;
        let providerModels = {};
        
        try {
          // Try to parse with the proper TOML parser first
          const parsedToml = TOML.parse(providerModelsContent, { joiner: '\n', bigint: false });
          
          if (typeof parsedToml === 'object' && parsedToml !== null) {
            providerModels = Object.entries(parsedToml).reduce((acc, [key, value]) => {
              if (typeof value === 'object' && value !== null && Array.isArray((value as any).models)) {
                acc[key.toLowerCase()] = { models: (value as any).models };
              }
              return acc;
            }, {});
          }
        } catch (tomlError) {
          console.error('Error parsing TOML with parser, falling back to regex:', tomlError);
          
          // Fallback to regex-based parsing if TOML parser fails
          providerModels = {};
          const providerSections = providerModelsContent.split(/\[\w+\]/g).filter(Boolean);
          const sectionNames = providerModelsContent.match(/\[(\w+)\]/g);
          
          if (sectionNames) {
            sectionNames.forEach((name, index) => {
              const providerName = name.replace(/\[|\]/g, '');
              const section = providerSections[index];
              const modelsMatch = section.match(/models\s*=\s*\[([\s\S]*?)\]/);
              
              if (modelsMatch) {
                const modelsText = modelsMatch[1];
                const models = modelsText.match(/"([^"]+)"/g)?.map(m => m.replace(/"/g, '')) || [];
                providerModels[providerName] = { models };
              }
            });
          }
        }
        
        // Switch the model
        const providerConfig = aiConfigData.providers[providerName];
        const actualModel = this.conversationManager.switchAiClient(providerConfig, providerModels);
        
        // Send updated history after switching since it clears conversation
        const history = this.conversationManager.getHistory();
        this.io.emit('history-update', { 
          history: history.map(msg => ({
            role: msg._getType(),
            content: msg.content,
            hasToolCalls: msg.hasToolCalls,
            pendingToolCalls: msg.pendingToolCalls
          }))
        });
        
        // Emit model changed event
        this.io.emit('model-changed', { provider: providerName, model: actualModel });
        
        res.json({ provider: providerName, model: actualModel });
      } catch (error) {
        res.status(500).json({ error: `Failed to switch model: ${error}` });
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
    
    // API routes for inline configuration file editing
    this.app.get('/api/config/:file', (req, res) => {
      try {
        const { file } = req.params;
        
        // Only allow specific config files for security
        const allowedFiles = ['ai_config.json', 'servers.json', 'provider_models.toml'];
        
        if (!allowedFiles.includes(file)) {
          return res.status(403).json({ error: `Access to file "${file}" is not allowed` });
        }
        
        const filePath = path.join(__dirname, '../../../', file);
        
        if (!fs.existsSync(filePath)) {
          return res.status(404).json({ error: `File "${file}" not found` });
        }
        
        // Read the file content
        const content = fs.readFileSync(filePath, 'utf-8');
        
        // Return the content with file metadata
        res.json({
          file,
          path: filePath,
          content,
          // Add file type for editor syntax highlighting
          type: file.endsWith('.json') ? 'json' : file.endsWith('.toml') ? 'toml' : 'text'
        });
      } catch (error) {
        res.status(500).json({ error: `Failed to read configuration file: ${error}` });
      }
    });
    
    this.app.post('/api/config/:file', async (req, res) => {
      try {
        const { file } = req.params;
        const { content } = req.body;
        
        if (content === undefined) {
          return res.status(400).json({ error: 'File content is required' });
        }
        
        // Only allow specific config files for security
        const allowedFiles = ['ai_config.json', 'servers.json', 'provider_models.toml'];
        
        if (!allowedFiles.includes(file)) {
          return res.status(403).json({ error: `Access to file "${file}" is not allowed` });
        }
        
        const filePath = path.join(__dirname, '../../../', file);
        
        // Validate content based on file type
        if (file.endsWith('.json')) {
          try {
            // Check that it's valid JSON
            JSON.parse(content);
          } catch (jsonError) {
            return res.status(400).json({ error: `Invalid JSON: ${jsonError.message}` });
          }
        }
        
        // Save the file
        await fs.promises.writeFile(filePath, content, 'utf-8');
        
        // If this is the active config, notify that a restart might be needed
        const needsRestart = file !== 'provider_models.toml'; // provider_models.toml changes don't need restart
        
        // Return success
        res.json({ 
          success: true, 
          message: needsRestart 
            ? 'Configuration saved. A restart may be required for changes to take effect.' 
            : 'Configuration saved successfully.',
          needsRestart
        });
      } catch (error) {
        res.status(500).json({ error: `Failed to save configuration file: ${error}` });
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
      
      // Handle new conversation request
      socket.on('new-conversation', () => {
        try {
          this.conversationManager.newConversation();
          const history = this.conversationManager.getHistory();
          
          // Emit the new conversation ID
          const currentConversation = this.conversationManager.getCurrentConversation();
          this.io.emit('conversation-loaded', {
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
          console.error('Error creating new conversation:', error);
          socket.emit('error', { 
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
          console.error('Error loading conversation:', error);
          socket.emit('error', { 
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
      const toolsByServer = {};
      for (const serverName of serverNames) {
        try {
          const serverTools = this.serverManager.getServerTools(serverName);
          toolsByServer[serverName] = serverTools;
        } catch (error) {
          console.warn(`Error getting tools for server ${serverName}:`, error);
          toolsByServer[serverName] = [];
        }
      }
      
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
