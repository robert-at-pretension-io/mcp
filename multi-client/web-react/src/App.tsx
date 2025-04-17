import { Toaster } from 'react-hot-toast';
import Header from '@/components/Layout/Header';
import Footer from '@/components/Layout/Footer';
import MainLayout from '@/components/Layout/MainLayout';
import ChatArea from '@/components/Chat/ChatArea';
import Sidebar from '@/components/Sidebar/Sidebar';
import { useSocket } from '@/hooks/useSocket';
import { useStore } from '@/store/store';
import { useEffect } from 'react';
import { shallow } from 'zustand/shallow';
import ModelModal from '@/components/Modals/ModelModal';
import ServersModal from '@/components/Modals/ServersModal';
import ConfigEditorModal from '@/components/Modals/ConfigEditorModal';

function App() {
  // Initialize socket connection and event listeners
  useSocket();

  // Fetch initial data (conversations, providers)
  // Correct usage: Pass shallow as the second argument to useStore
  const { fetchInitialData, isModelModalOpen, isServersModalOpen, isConfigEditorOpen } = useStore(
    (state: StoreType) => ({ // Type state
      fetchInitialData: state.fetchInitialData,
      isModelModalOpen: state.isModelModalOpen,
      isServersModalOpen: state.isServersModalOpen,
      isConfigEditorOpen: state.isConfigEditorOpen,
    }),
    shallow
  );

  useEffect(() => {
    fetchInitialData();
  }, [fetchInitialData]);


  return (
    <div className="flex flex-col min-h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100">
      <Header />
      <MainLayout>
        <ChatArea />
        <Sidebar />
      </MainLayout>
      <Footer />
      <Toaster position="bottom-right" />

      {/* Modals */}
      {isModelModalOpen && <ModelModal />}
      {isServersModalOpen && <ServersModal />}
      {isConfigEditorOpen && <ConfigEditorModal />}
    </div>
  );
}

export default App;
