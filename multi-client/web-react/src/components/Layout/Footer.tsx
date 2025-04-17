import React from 'react';
import { useStore, StoreType } from '@/store/store'; // Import StoreType

const Footer: React.FC = () => {
  const statusMessage = useStore((state: StoreType) => state.statusMessage); // Type state

  return (
    <footer className="bg-white dark:bg-gray-800 py-4 px-6 shadow-inner border-t border-gray-200 dark:border-gray-700">
      <div className="container mx-auto text-center text-gray-500 dark:text-gray-400">
        <div id="status" className="font-medium">
          {statusMessage}
        </div>
      </div>
    </footer>
  );
};

export default Footer;
