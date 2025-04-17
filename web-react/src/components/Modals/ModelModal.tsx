import React, { useState, useEffect, useCallback } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faTimes, faEye, faEyeSlash } from '@fortawesome/free-solid-svg-icons';
import { useStore } from '@/store/store';
import { shallow } from 'zustand/shallow';
import toast from 'react-hot-toast';
import Spinner from '@/components/common/Spinner'; // Assuming Spinner component exists

const ModelModal: React.FC = () => {
    const {
        isModelModalOpen,
        closeModelModal,
        providers,
        providerModels,
        currentProvider,
        updateApiKey,
        switchProviderAndModel,
        fetchProviders, // To refresh state after changes
    } = useStore(
        (state) => ({
            isModelModalOpen: state.isModelModalOpen,
            closeModelModal: state.closeModelModal,
            providers: state.providers,
            providerModels: state.providerModels,
            currentProvider: state.currentProvider,
            updateApiKey: state.updateApiKey,
            switchProviderAndModel: state.switchProviderAndModel,
            fetchProviders: state.fetchProviders,
        }),
        shallow
    );

    const [selectedProvider, setSelectedProvider] = useState<string>(currentProvider);
    const [selectedModel, setSelectedModel] = useState<string>('');
    const [apiKey, setApiKey] = useState<string>('');
    const [isApiKeyVisible, setIsApiKeyVisible] = useState<boolean>(false);
    const [isLoading, setIsLoading] = useState<boolean>(false);
    const [initialProvider, setInitialProvider] = useState<string>('');
    const [initialModel, setInitialModel] = useState<string>('');

    // Initialize local state when modal opens or currentProvider changes
    useEffect(() => {
        if (isModelModalOpen) {
            setSelectedProvider(currentProvider);
            const currentModel = providers[currentProvider]?.model || '';
            setSelectedModel(currentModel);
            setInitialProvider(currentProvider);
            setInitialModel(currentModel);
            setApiKey(''); // Clear API key field on open
            setIsApiKeyVisible(false); // Reset visibility
        }
    }, [isModelModalOpen, currentProvider, providers]);

    // Update model options when selected provider changes
    const availableModels = providerModels[selectedProvider?.toLowerCase()]?.models || [];
    const configuredModel = providers[selectedProvider]?.model;

    useEffect(() => {
        // Set initial model selection based on available/configured
        if (configuredModel) {
            setSelectedModel(configuredModel);
        } else if (availableModels.length > 0) {
            setSelectedModel(availableModels[0]);
        } else {
            setSelectedModel('');
        }
    }, [selectedProvider, availableModels, configuredModel]);


    const handleProviderChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
        setSelectedProvider(e.target.value);
        // Model selection will update via useEffect
    };

    const handleModelChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
        setSelectedModel(e.target.value);
    };

    const handleApiKeyChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        setApiKey(e.target.value);
    };

    const toggleApiKeyVisibility = () => {
        setIsApiKeyVisible(!isApiKeyVisible);
    };

    const handleApply = useCallback(async () => {
        setIsLoading(true);
        let success = true;
        let providerSwitched = false;

        const providerChanged = selectedProvider !== initialProvider;
        const modelChanged = selectedModel !== initialModel;
        const apiKeyProvided = apiKey.trim() !== '';

        if (!selectedProvider) {
            toast.error('Please select a provider.');
            setIsLoading(false);
            return;
        }
        if (!selectedModel) {
            toast.error('Please select a model.');
            setIsLoading(false);
            return;
        }

        if (!providerChanged && !modelChanged && !apiKeyProvided) {
            toast.success('No changes to apply.');
            closeModelModal();
            setIsLoading(false);
            return;
        }

        try {
            // 1. Update API Key if provided
            if (apiKeyProvided) {
                await updateApiKey(selectedProvider, apiKey.trim());
                // If the key was updated for the *currently active* provider,
                // the socket event 'model-changed' should handle the client switch.
                // We might need to refetch providers here to update the apiKeySet status visually.
                await fetchProviders(); // Refresh provider state
                if (selectedProvider === currentProvider) {
                    providerSwitched = true; // Assume socket will handle the switch
                }
            }

            // 2. Switch provider/model if changed and not already handled by API key update
            if ((providerChanged || modelChanged) && !(apiKeyProvided && selectedProvider === currentProvider)) {
                await switchProviderAndModel(selectedProvider, selectedModel);
                providerSwitched = true;
                // Success toast/UI updates handled by socket event listener
            }

        } catch (error) {
            success = false;
            // Error toast is handled by the API service/store action
        } finally {
            setIsLoading(false);
            if (success) {
                closeModelModal();
            }
        }
    }, [
        selectedProvider, selectedModel, apiKey, initialProvider, initialModel,
        updateApiKey, switchProviderAndModel, closeModelModal, fetchProviders, currentProvider
    ]);

    if (!isModelModalOpen) return null;

    const isKeySet = providers[selectedProvider]?.apiKeySet ?? false;

    return (
        <div className="modal open"> {/* Add 'open' class */}
            <div className="modal-content max-w-md w-full"> {/* Add 'w-full' */}
                {isLoading && <Spinner message="Applying changes..." />}
                <div className="flex justify-between items-center mb-5">
                    <h2 className="text-xl font-bold text-gray-800 dark:text-white">Change AI Provider/Model</h2>
                    <button onClick={closeModelModal} className="modal-close" title="Close">
                        <FontAwesomeIcon icon={faTimes} />
                    </button>
                </div>

                <div className="space-y-6">
                    {/* Provider Select */}
                    <div className="space-y-2">
                        <label htmlFor="provider-select" className="block text-sm font-medium text-gray-700 dark:text-gray-300">Select Provider</label>
                        <select
                            id="provider-select"
                            value={selectedProvider}
                            onChange={handleProviderChange}
                            disabled={isLoading}
                            className="w-full p-2.5 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-800 dark:text-white focus:ring-2 focus:ring-primary"
                        >
                            <option value="" disabled>-- Select Provider --</option>
                            {Object.keys(providers).sort().map(name => (
                                <option key={name} value={name}>{name}</option>
                            ))}
                        </select>
                    </div>

                    {/* Model Select */}
                    <div className="space-y-2">
                        <label htmlFor="model-select" className="block text-sm font-medium text-gray-700 dark:text-gray-300">Select Model</label>
                        <select
                            id="model-select"
                            value={selectedModel}
                            onChange={handleModelChange}
                            disabled={isLoading || !selectedProvider}
                            className="w-full p-2.5 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-800 dark:text-white focus:ring-2 focus:ring-primary"
                        >
                            <option value="" disabled>-- Select Model --</option>
                            {/* Add configured model if not in suggestions */}
                            {configuredModel && !availableModels.includes(configuredModel) && (
                                <option key={configuredModel} value={configuredModel}>
                                    {configuredModel} (from config)
                                </option>
                            )}
                            {/* Add suggested models */}
                            {availableModels.map(model => (
                                <option key={model} value={model}>{model}</option>
                            ))}
                            {/* Handle no models case */}
                            {availableModels.length === 0 && !configuredModel && (
                                <option value="" disabled>No models available/suggested</option>
                            )}
                        </select>
                    </div>

                    {/* API Key Input */}
                    <div className="space-y-2">
                        <label htmlFor="api-key-input" className="block text-sm font-medium text-gray-700 dark:text-gray-300">API Key</label>
                        <div className="relative">
                            <input
                                type={isApiKeyVisible ? 'text' : 'password'}
                                id="api-key-input"
                                value={apiKey}
                                onChange={handleApiKeyChange}
                                placeholder={isKeySet ? 'API Key is set (enter new to overwrite)' : 'Enter API key for this provider'}
                                disabled={isLoading}
                                className="w-full p-2.5 pr-10 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-800 dark:text-white focus:ring-2 focus:ring-primary"
                            />
                            <button
                                id="toggle-api-key-visibility"
                                onClick={toggleApiKeyVisibility}
                                className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
                                title="Toggle visibility"
                                type="button"
                            >
                                <FontAwesomeIcon icon={isApiKeyVisible ? faEyeSlash : faEye} />
                            </button>
                        </div>
                        <div className="text-xs text-gray-500 dark:text-gray-400 italic mt-1">
                            API keys are saved directly in the configuration file (ai_config.json).
                        </div>
                    </div>

                    {/* Action Buttons */}
                    <div className="flex justify-end gap-3 pt-4 border-t border-gray-200 dark:border-gray-700">
                        <button
                            id="cancel-model-change"
                            onClick={closeModelModal}
                            disabled={isLoading}
                            className="btn btn-secondary py-2 px-4"
                            type="button"
                        >
                            Cancel
                        </button>
                        <button
                            id="apply-model-change"
                            onClick={handleApply}
                            disabled={isLoading}
                            className="btn btn-primary py-2 px-4"
                            type="button"
                        >
                            {isLoading ? 'Applying...' : 'Apply'}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
};

export default ModelModal;
