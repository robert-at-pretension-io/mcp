import { BaseChatModel } from '@langchain/core/language_models/chat_models';
import type { IAiClient } from './IAiClient.js';
import type { ConversationMessage } from '../conversation/Message.js';
export declare class LangchainClient implements IAiClient {
    private chatModel;
    private modelIdentifier;
    constructor(chatModel: BaseChatModel, modelIdentifier: string);
    generateResponse(messages: ConversationMessage[]): Promise<string>;
    getModelName(): string;
}
