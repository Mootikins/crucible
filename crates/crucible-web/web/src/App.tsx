import { Component, createMemo } from 'solid-js';
import { ProjectProvider } from '@/contexts/ProjectContext';
import { SessionProvider, useSession } from '@/contexts/SessionContext';
import { ChatProvider } from '@/contexts/ChatContext';
import { WhisperProvider } from '@/contexts/WhisperContext';
import { SettingsProvider } from '@/contexts/SettingsContext';
import { EditorProvider } from '@/contexts/EditorContext';
import { DockLayout, ArkLayout, ChatContent } from '@/components';

const ChatWithSession: Component = () => {
  const { currentSession, setSessionTitle } = useSession();
  
  const useArkLayout = createMemo(() => {
    const params = new URLSearchParams(window.location.search);
    return params.get('layout') === 'ark';
  });
  
  return (
    <ChatProvider session={currentSession} setSessionTitle={setSessionTitle}>
      {useArkLayout() ? (
        <ArkLayout chatContent={ChatContent} />
      ) : (
        <DockLayout chatContent={ChatContent} />
      )}
    </ChatProvider>
  );
};

const App: Component = () => {
  return (
    <SettingsProvider>
      <ProjectProvider>
        <EditorProvider>
          <SessionProvider>
            <WhisperProvider>
              <ChatWithSession />
            </WhisperProvider>
          </SessionProvider>
        </EditorProvider>
      </ProjectProvider>
    </SettingsProvider>
  );
};

export default App;
