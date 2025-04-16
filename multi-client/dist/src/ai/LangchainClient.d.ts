import type { RunnableInterface } from "@langchain/core/runnables";
import type { BaseLanguageModelInput } from "@langchain/core/language_models/base";
import type { BaseMessageChunk } from "@langchain/core/messages";
import type { IAiClient } from './IAiClient.js';
import type { ConversationMessage } from '../conversation/Message.js';
export declare class LangchainClient implements IAiClient {
    private chatModel;
    private modelIdentifier;
    private providerName;
    constructor(chatModel: RunnableInterface<BaseLanguageModelInput, BaseMessageChunk>, modelIdentifier: string, providerName?: string);
    generateResponse(messages: ConversationMessage[]): Promise<string>;
    getModelName(): string;
    getProvider(): string;
}
