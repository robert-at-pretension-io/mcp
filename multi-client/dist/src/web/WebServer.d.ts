export declare class WebServer {
    private app;
    private server;
    private io;
    private conversationManager;
    private serverManager;
    private port;
    private isRunning;
    constructor(conversationManager: any, serverManager: any, port?: number);
    private setupRoutes;
    private setupSocketEvents;
    private sendInitialData;
    private processUserMessage;
    /**
     * Start the web server
     */
    start(): void;
    /**
     * Stop the web server
     */
    stop(): Promise<unknown>;
}
