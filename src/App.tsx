import { useState } from "react";
import { OverviewView, SessionsView, SynchronizationView } from "./components/AppViews";
import { Sidebar } from "./components/Sidebar";
import { useAppController } from "./hooks/useAppController";
import { isDesktopApp } from "./lib/desktop";
import {
  activeAttempt,
  canAnimateSlot,
  canPlaySlot,
  projectPromptTotal,
  remoteStatusLabel,
  sectionTitles,
  slotStatusLabel,
  stageCopy,
} from "./lib/app-state";

function AccessGate({
  checking,
  busy,
  error,
  token,
  onToken,
  onSubmit,
}: {
  checking: boolean;
  busy: boolean;
  error: string;
  token: string;
  onToken: (value: string) => void;
  onSubmit: () => void;
}) {
  const [showKey, setShowKey] = useState(false);

  if (checking) {
    return (
      <main className="access-gate">
        <div className="access-brand">
          <img src="/assets/logo.svg" alt="" />
          <span><strong>FlowContent</strong><small>AUTO</small></span>
        </div>
        <section className="access-panel">
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: "14px", padding: "32px 0" }}>
            <div
              style={{
                width: 36,
                height: 36,
                borderRadius: "50%",
                border: "2px solid var(--accent, #3b82f6)",
                borderTopColor: "transparent",
                animation: "spin 0.8s linear infinite",
              }}
            />
            <p style={{ color: "var(--text-muted, #71717a)", fontSize: "13px", margin: 0 }}>Verificando licença salva...</p>
          </div>
          <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
        </section>
      </main>
    );
  }

  return (
    <main className="access-gate">
      <div className="access-brand">
        <img src="/assets/logo.svg" alt="" />
        <span><strong>FlowContent</strong><small>AUTO</small></span>
      </div>
      <section className="access-panel">
        <span className="access-signal">▶</span>
        <span className="section-kicker">ACESSO PROTEGIDO</span>
        <h1>Desbloquear central</h1>
        <p>Informe a chave de licença para acessar produções, arquivos locais e a ponte com o Flow.</p>
        <label>
          <span>Chave de licença</span>
          <div style={{ position: "relative" }}>
            <input
              autoFocus
              type={showKey ? "text" : "password"}
              value={token}
              disabled={busy}
              onChange={(event) => onToken(event.target.value)}
              onKeyDown={(event) => event.key === "Enter" && onSubmit()}
              placeholder="CF-XXXXX-XXXXX-XXXX"
              style={{ fontFamily: "monospace", letterSpacing: "0.05em", paddingRight: "40px" }}
            />
            <button
              type="button"
              onClick={() => setShowKey(!showKey)}
              style={{
                position: "absolute",
                right: 10,
                top: "50%",
                transform: "translateY(-50%)",
                background: "none",
                border: "none",
                cursor: "pointer",
                color: "var(--text-muted, #71717a)",
                padding: 0,
                fontSize: "13px",
                lineHeight: 1,
              }}
              title={showKey ? "Ocultar chave" : "Mostrar chave"}
            >
              {showKey ? "🚫" : "👁️"}
            </button>
          </div>
        </label>
        {error && <div className="access-error">{error}</div>}
        <button className="dispatch-button wide" disabled={!token.trim() || busy || !isDesktopApp()} onClick={onSubmit}>
          <span className="mini-play">▶</span>
          {busy ? "Validando chave..." : isDesktopApp() ? "Ativar licença" : "Abra o aplicativo desktop"}
        </button>
        <div className="access-foot">
          <span><i /> Chave salva localmente</span>
          <span><i /> Expira dia 1° de cada mês</span>
          <span><i /> Backend bloqueado</span>
        </div>
        <p style={{ textAlign: "center", fontSize: "11px", color: "var(--text-muted, #52525b)", margin: "8px 0 0", lineHeight: 1.5 }}>
          Sua chave expira no dia 1° de cada mês.<br />
          Uma nova chave será disponibilizada na área de membros.
        </p>
      </section>
    </main>
  );
}

export default function App() {
  const {
    activeSection,
    setActiveSection,
    projects,
    selected,
    setSelectedId,
    desktopReady,
    busy,
    updateStatus,
    availableUpdate,
    toast,
    createOpen,
    setCreateOpen,
    deleteTarget,
    setDeleteTarget,
    title,
    setTitle,
    assetSrtMode,
    setAssetSrtMode,
    assetSrtValue,
    setAssetSrtValue,
    transitionMode,
    setTransitionMode,
    assetOutputDir,
    generationConcurrency,
    setGenerationConcurrency,
    promptText,
    setPromptText,
    promptCount,
    generationMode,
    setGenerationMode,
    generationSettings,
    setGenerationSettings,
    selectedGenerationProgress,
    authenticated,
    authChecking,
    authToken,
    setAuthToken,
    authError,
    bridgeStatus,
    assemblyStatus,
    visibleSlots,
    selectedDownloadedAssets,
    readyLocalSlotCount,
    resumableGeneration,
    animatableSourceOrders,
    activeVideoSlot,
    setActiveVideoSlot,
    inlineImageSrc,
    failedRemoteAssets,
    videoPlaybackSrc,
    videoThumbnailSrc,
    pendingAudioByProject,
    handleAuthSubmit,
    handleLock,
    handleCheckForUpdates,
    handleInstallUpdate,
    handleCreate,
    handleDelete,
    handleChooseAssetOutputDir,
    handleChooseAudio,
    handleAudio,
    refreshBridge,
    handleOpenFlowBrowser,
    handlePrompts,
    handleDownloadSrt,
    handleExportCapcut,
    handleContinueGeneration,
    handlePauseGeneration,
    handleRestartGeneration,
    handleRetry,
    handleAnimateAll,
    handleAnimateSlot,
    handleRefreshSlotAsset,
    handleRetrySlot,
    handleSaveAssemblyKeys,
    handleClearAssemblyKeys,
    markLocalAssetFailure,
    markRemoteAssetFailure,
    markVideoThumbnailFailure,
    ensureVideoPlaybackSrc,
  } = useAppController();

  if (!authenticated) {
    return (
      <AccessGate
        checking={authChecking}
        busy={busy === "auth"}
        error={authError}
        token={authToken}
        onToken={setAuthToken}
        onSubmit={handleAuthSubmit}
      />
    );
  }

  return (
    <>
      <div className="app-shell">
        <Sidebar
          activeSection={activeSection}
          projectCount={projects.length}
          desktopReady={desktopReady}
          bridgeConnected={bridgeStatus.extensionConnected}
          onNavigate={(section) => setActiveSection(section)}
        />

        <main>
          <header className="topbar">
            <div>
              <div className="eyebrow"><span>FLOWCONTENT AUTO</span> / CENTRAL LOCAL</div>
              <h1>{sectionTitles[activeSection] ?? "FlowContent Auto"}</h1>
            </div>
            <div className="topbar-actions">
              {updateStatus?.configured && (
                <button
                  className="secondary-button"
                  disabled={!isDesktopApp() || busy === "update-check" || busy === "update-install"}
                  onClick={availableUpdate ? handleInstallUpdate : () => void handleCheckForUpdates()}
                >
                  {busy === "update-install"
                    ? "Atualizando..."
                    : busy === "update-check"
                      ? "Buscando update..."
                      : availableUpdate
                        ? `Atualizar ${availableUpdate.version}`
                        : "Verificar atualização"}
                </button>
              )}
              <span className={`runtime-chip ${desktopReady ? "ready" : ""}`}>
                <i /> {desktopReady ? "Desktop conectado" : "Prévia web"}
              </span>
              <button className="icon-button lock-button" aria-label="Bloquear aplicativo" onClick={handleLock}>⌁</button>
              <button className="secondary-button" disabled={!isDesktopApp()} onClick={() => setCreateOpen(true)}>
                + Nova produção
              </button>
            </div>
          </header>

          {activeSection === "central" && (
            <OverviewView
              projects={projects}
              bridgeConnected={bridgeStatus.extensionConnected}
              onNavigate={setActiveSection}
              onSelect={setSelectedId}
              onCreate={() => setCreateOpen(true)}
            />
          )}

          {activeSection === "producoes" && <div className="production-layout">
            <aside className="project-library">
              <div className="library-head">
                <span className="section-kicker">BIBLIOTECA LOCAL</span>
                <strong>{projects.length} produções</strong>
              </div>
              <div className="project-list">
                {projects.map((project) => (
                  <button
                    className={`project-card ${selected?.localProjectId === project.localProjectId ? "selected" : ""}`}
                    key={project.localProjectId}
                    onClick={() => setSelectedId(project.localProjectId)}
                  >
                    <span className="project-play">▶</span>
                    <span>
                      <strong>{project.title}</strong>
                      <small>{stageCopy[project.stage]?.title ?? project.stage}</small>
                    </span>
                    <em>{String(projectPromptTotal(project)).padStart(2, "0")}</em>
                  </button>
                ))}
                {!projects.length && (
                  <div className="library-empty">
                    <span>▶</span>
                    <strong>Nenhuma produção</strong>
                    <p>Crie uma produção para organizar áudio, SRTs e prompts.</p>
                  </div>
                )}
              </div>
              <div className="library-note">
                <strong>Arquivos sob seu controle</strong>
                <p>Áudio, SRTs e prompts ficam na pasta local da produção.</p>
              </div>
            </aside>

            <section className="production-workspace">
              {!selected ? (
                <div className="workspace-empty">
                  <span className="empty-play">▶</span>
                  <span className="section-kicker">PRIMEIRO PASSO</span>
                  <h2>Crie sua primeira produção</h2>
                  <p>Ela receberá uma pasta local e poderá ser vinculada ao ID de um projeto no Flow.</p>
                  <button className="dispatch-button" disabled={!desktopReady} onClick={() => setCreateOpen(true)}>
                    <span className="mini-play">▶</span> Criar produção
                  </button>
                </div>
              ) : (
                <>
                  <div className="production-title">
                    <div>
                      <span className="section-kicker">PRODUÇÃO</span>
                      <h2>{selected.title}</h2>
                      <p>{projectPromptTotal(selected)} prompts esperados</p>
                      <p>Pasta final dos assets: {selected.assetOutputDir}</p>
                    </div>
                    <div className="production-title-actions">
                      <div className="flow-link-state">
                        <small>PROJETO FLOW</small>
                        <strong>{selected.flowProjectId ? "Vinculado automaticamente" : bridgeStatus.extensionConnected ? "Criando projeto..." : "Aguardando ponte"}</strong>
                        <span>{selected.flowProjectId ?? "O navegador Flow pode permanecer minimizado"}</span>
                      </div>
                      <button className="danger-button" onClick={() => setDeleteTarget(selected)}>Excluir produção</button>
                    </div>
                  </div>

                  {!selected.assetCount ? (
                    <div className="production-flow">
                      <section className="workflow-panel prompts-panel">
                        <div className="panel-heading">
                          <div>
                            <span className="section-kicker">01 / ENTRADA</span>
                            <h3>Prompts sem áudio</h3>
                          </div>
                          <span className="count-match">{promptCount} prompts</span>
                        </div>
                        <p className="panel-description">Cole um prompt por linha para criar os slots direto no app, ou use a sincronização por áudio se quiser que o app monte os intervalos para você.</p>
                        <textarea
                          value={promptText}
                          onChange={(event) => setPromptText(event.target.value)}
                          placeholder={"Prompt 01...\nPrompt 02...\nPrompt 03..."}
                        />
                        <div className="prerequisite-actions">
                          <button className="secondary-button" onClick={() => setActiveSection("sincronizacao")}>Usar fluxo com áudio</button>
                          <button className="dispatch-button" disabled={!promptCount || busy === "prompts"} onClick={handlePrompts}>
                            <span className="mini-play">▶</span>
                            {busy === "prompts" ? "Salvando prompts..." : "Salvar prompts diretos"}
                          </button>
                        </div>
                      </section>
                    </div>
                  ) : (
                    <div className="production-flow">
                      <section className="workflow-panel prompts-panel">
                        <div className="panel-heading">
                          <div>
                            <span className="section-kicker">01 / PROMPTS</span>
                            <h3>Cole um prompt por linha</h3>
                          </div>
                          <span className={`count-match ${promptCount === selected.assetCount && selected.assetCount > 0 ? "matched" : ""}`}>
                            {promptCount} / {selected.assetCount}
                          </span>
                        </div>
                        <p className="panel-description">A ordem das linhas será a ordem de geração dos assets.</p>
                        <textarea
                          value={promptText}
                          onChange={(event) => setPromptText(event.target.value)}
                          placeholder={"Prompt para o primeiro trecho...\nPrompt para o segundo trecho..."}
                        />
                        <button className="secondary-button wide" disabled={!selected.assetCount || busy === "prompts"} onClick={handlePrompts}>
                          {busy === "prompts" ? "Salvando prompts..." : selected.promptCount ? "Atualizar prompts" : "Salvar prompts"}
                        </button>
                        <button
                          className="secondary-button wide"
                          disabled={
                            !selected.assetCount
                            || readyLocalSlotCount < selected.assetCount
                            || busy === "capcut"
                            || busy === "generate"
                            || busy === "animate"
                            || busy.startsWith("animate-")
                            || busy.startsWith("retry-")
                            || (selectedGenerationProgress?.active ?? false)
                          }
                          onClick={handleExportCapcut}
                        >
                          {busy === "capcut" ? "Exportando para o CapCut..." : "Criar projeto no CapCut"}
                        </button>
                        {selected.assetCount > 0 && readyLocalSlotCount < selected.assetCount && (
                          <p className="generation-help">
                            O draft do CapCut será liberado quando todos os {selected.assetCount} slots estiverem salvos localmente.
                          </p>
                        )}
                      </section>

                      <section className="workflow-panel generation-panel">
                        <div className="panel-heading">
                          <div>
                            <span className="section-kicker">02 / GERAÇÃO</span>
                            <h3>Escolha o resultado</h3>
                          </div>
                          <span className={`panel-state ${bridgeStatus.extensionConnected && selected.flowProjectId ? "done" : ""}`}>
                            {selected.flowProjectId ? "FLOW VINCULADO" : "AGUARDANDO FLOW"}
                          </span>
                        </div>
                        <div className="generation-modes">
                          {([
                            ["IMAGE", "Somente imagens", "Gera uma imagem para cada prompt."],
                            ["VIDEO", "Vídeos diretos", "Gera cada vídeo diretamente do prompt."],
                            ["IMAGE_TO_VIDEO", "Imagens + animação", "Gera imagens e depois anima cada uma."],
                          ] as const).map(([mode, label, description]) => (
                            <button className={generationMode === mode ? "active" : ""} key={mode} onClick={() => setGenerationMode(mode)}>
                              <strong>{label}</strong><small>{description}</small>
                            </button>
                          ))}
                        </div>

                        <div className="generation-settings">
                          <label className="gen-setting">
                            <span>Gerações simultâneas</span>
                            <input
                              type="number"
                              min={1}
                              max={4}
                              value={generationConcurrency}
                              onChange={(e) => setGenerationConcurrency(Math.max(1, Math.min(4, Number(e.target.value) || 1)))}
                            />
                          </label>
                          <span className="section-kicker">CONFIGURAÇÕES</span>
                          {(generationMode === "IMAGE" || generationMode === "IMAGE_TO_VIDEO") && (
                            <label className="gen-setting">
                              <span>Modelo de imagem</span>
                              <select value={generationSettings.imageModel} onChange={(e) => setGenerationSettings((s) => ({ ...s, imageModel: e.target.value }))}>
                                <option value="GEM_PIX_2">Nanobanana 2</option>
                                <option value="NARWHAL">NanoBananaPro</option>
                              </select>
                            </label>
                          )}
                          {(generationMode === "IMAGE" || generationMode === "IMAGE_TO_VIDEO") && (
                            <label className="gen-setting">
                              <span>Formato da imagem</span>
                              <div className="ratio-buttons">
                                {(["IMAGE_ASPECT_RATIO_LANDSCAPE", "IMAGE_ASPECT_RATIO_PORTRAIT", "IMAGE_ASPECT_RATIO_SQUARE"] as const).map((ratio) => (
                                  <button key={ratio} className={generationSettings.imageAspectRatio === ratio ? "active" : ""} onClick={() => setGenerationSettings((s) => ({ ...s, imageAspectRatio: ratio }))}>
                                    {ratio === "IMAGE_ASPECT_RATIO_LANDSCAPE" ? "16:9" : ratio === "IMAGE_ASPECT_RATIO_PORTRAIT" ? "9:16" : "1:1"}
                                  </button>
                                ))}
                              </div>
                            </label>
                          )}
                          {(generationMode === "VIDEO" || generationMode === "IMAGE_TO_VIDEO") && (
                            <label className="gen-setting">
                              <span>Formato do vídeo</span>
                              <div className="ratio-buttons">
                                {(["VIDEO_ASPECT_RATIO_LANDSCAPE", "VIDEO_ASPECT_RATIO_PORTRAIT", "VIDEO_ASPECT_RATIO_SQUARE"] as const).map((ratio) => (
                                  <button key={ratio} className={generationSettings.videoAspectRatio === ratio ? "active" : ""} onClick={() => setGenerationSettings((s) => ({ ...s, videoAspectRatio: ratio }))}>
                                    {ratio === "VIDEO_ASPECT_RATIO_LANDSCAPE" ? "16:9" : ratio === "VIDEO_ASPECT_RATIO_PORTRAIT" ? "9:16" : "1:1"}
                                  </button>
                                ))}
                              </div>
                            </label>
                          )}
                        </div>

                        {selectedGenerationProgress && (selectedGenerationProgress.active || selectedGenerationProgress.paused) && (
                          <div className="generation-progress">
                            <div className="progress-header">
                              <span className="section-kicker">PROGRESSO</span>
                              <strong>{selectedGenerationProgress.completedPrompts} / {selectedGenerationProgress.totalPrompts}</strong>
                            </div>
                            <div className="progress-bar">
                              <div className="progress-fill" style={{ width: `${selectedGenerationProgress.totalPrompts > 0 ? (selectedGenerationProgress.completedPrompts / selectedGenerationProgress.totalPrompts) * 100 : 0}%` }} />
                            </div>
                            <small>
                              {selectedGenerationProgress.paused
                                ? "Fila pausada."
                                : `${selectedGenerationProgress.inFlight} em voo, próximo índice ${Math.min(selectedGenerationProgress.currentIndex + 1, selectedGenerationProgress.totalPrompts)} de ${selectedGenerationProgress.totalPrompts}`}
                            </small>
                          </div>
                        )}

                        {selectedGenerationProgress?.active && !selectedGenerationProgress.paused && (
                          <button
                            className="quiet-button"
                            disabled={busy === "pause-generation" || busy === "generate" || busy === "animate"}
                            onClick={handlePauseGeneration}
                          >
                            {busy === "pause-generation" ? "Pausando..." : "Pausar geração"}
                          </button>
                        )}

                        {!selectedGenerationProgress?.active && resumableGeneration.resumable && (
                          <div className="generation-progress">
                            <div className="progress-header">
                              <span className="section-kicker">RETOMADA</span>
                              <strong>{resumableGeneration.completed} / {resumableGeneration.total}</strong>
                            </div>
                            <div className="progress-bar">
                              <div className="progress-fill" style={{ width: `${resumableGeneration.total > 0 ? (resumableGeneration.completed / resumableGeneration.total) * 100 : 0}%` }} />
                            </div>
                            <small>
                              {resumableGeneration.remaining} restante{resumableGeneration.remaining === 1 ? "" : "s"}
                              {resumableGeneration.failed > 0 ? `, ${resumableGeneration.failed} com falha` : ""}
                              {resumableGeneration.processing > 0 ? `, ${resumableGeneration.processing} em processamento` : ""}
                              . A fila pode continuar daqui.
                            </small>
                          </div>
                        )}

                        {selectedGenerationProgress && selectedGenerationProgress.failedSlots.length > 0 && (
                          <div className="generation-failures">
                            <span className="section-kicker danger-kicker">FALHAS ({selectedGenerationProgress.failedSlots.length})</span>
                            <div className="failure-list">
                              {selectedGenerationProgress.failedSlots.map((slot, idx) => (
                                <div key={`${slot.sourceOrder}-${idx}`} className="failure-item">
                                  <span>Slot {slot.sourceOrder}</span>
                                  <span>{slot.error}</span>
                                </div>
                              ))}
                            </div>
                            <button className="secondary-button" disabled={busy === "generate"} onClick={handleRetry}>
                              {busy === "generate" ? "Retentando..." : `Retentar ${selectedGenerationProgress.failedSlots.length} com falha`}
                            </button>
                          </div>
                        )}
                        <button
                          className="secondary-button wide"
                          disabled={
                            !selected.promptCount
                            || !selected.flowProjectId
                            || !bridgeStatus.extensionConnected
                            || busy === "generate"
                            || busy === "pause-generation"
                            || busy === "animate"
                            || busy.startsWith("animate-")
                            || ((selectedGenerationProgress?.active ?? false) && !selectedGenerationProgress?.paused)
                            || !resumableGeneration.resumable
                          }
                          onClick={handleContinueGeneration}
                        >
                          {busy === "generate"
                            ? "Enviando para o Flow..."
                            : resumableGeneration.resumable
                              ? `Continuar geracao (${resumableGeneration.completed}/${resumableGeneration.total})`
                              : "Continuar geracao"}
                        </button>
                        <button
                          className="dispatch-button"
                          disabled={
                            !selected.promptCount
                            || !selected.flowProjectId
                            || !bridgeStatus.extensionConnected
                            || busy === "generate"
                            || busy === "pause-generation"
                            || busy === "animate"
                            || busy.startsWith("animate-")
                            || ((selectedGenerationProgress?.active ?? false) && !selectedGenerationProgress?.paused)
                          }
                          onClick={handleRestartGeneration}
                        >
                          <span className="mini-play">{">"}</span>
                          {busy === "generate" ? "Enviando para o Flow..." : "Gerar do zero"}
                        </button>
                        {!selected.flowProjectId && <p className="generation-help">O projeto Flow será criado e vinculado automaticamente assim que a ponte estiver conectada.</p>}
                      </section>
                    </div>
                  )}

                  {(visibleSlots.length > 0 || selectedDownloadedAssets.length > 0) && (
                    <section className="workflow-panel assets-panel">
                      <div className="panel-heading">
                        <div>
                          <span className="section-kicker">03 / ASSETS GERADOS</span>
                          <h3>Slots acompanhados em tempo real</h3>
                        </div>
                        <div className="assets-panel-actions">
                          <button
                            className="secondary-button asset-batch-button"
                            disabled={
                              animatableSourceOrders.length === 0
                              || !selected?.flowProjectId
                              || !bridgeStatus.extensionConnected
                              || busy === "animate"
                              || busy === "generate"
                              || busy.startsWith("animate-")
                              || (selectedGenerationProgress?.active ?? false)
                            }
                            onClick={handleAnimateAll}
                          >
                            {busy === "animate" ? "Animando..." : `Animar todas (${animatableSourceOrders.length})`}
                          </button>
                          <span className="count-match matched">{visibleSlots.length || selectedDownloadedAssets.length} slot{(visibleSlots.length || selectedDownloadedAssets.length) !== 1 ? "s" : ""}</span>
                        </div>
                      </div>
                      <div className="asset-grid">
                        {visibleSlots.map((slot) => (
                          <div className={`asset-card slot-${slot.status}`} key={`${slot.sourceOrder}-${slot.localPath ?? slot.prompt}`}>
                            <div className="asset-thumbnail">
                              {slot.localPath ? (() => {
                                const inlineSrc = inlineImageSrc[slot.localPath];
                                if (slot.currentFileType === "image") {
                                  const remoteFallbackSrc = slot.remoteUrl && !failedRemoteAssets[slot.remoteUrl] ? slot.remoteUrl : null;
                                  const assetSrc = inlineSrc ?? remoteFallbackSrc;
                                  if (!assetSrc) {
                                    return (
                                      <div className="asset-placeholder">
                                        <strong>{String(slot.sourceOrder).padStart(2, "0")}</strong>
                                        <small>Carregando preview</small>
                                      </div>
                                    );
                                  }
                                  return (
                                    <img
                                      src={assetSrc}
                                      alt={`Slot ${slot.sourceOrder}`}
                                      loading="lazy"
                                      onError={() => inlineSrc ? markLocalAssetFailure(slot) : markRemoteAssetFailure(slot)}
                                    />
                                  );
                                }

                                const useRemoteVideo = Boolean(slot.remoteUrl && !failedRemoteAssets[slot.remoteUrl]);
                                const assetSrc = videoPlaybackSrc[slot.sourceOrder] ?? (useRemoteVideo ? slot.remoteUrl : null);
                                const thumbnailSrc = videoThumbnailSrc[slot.sourceOrder] ?? slot.thumbnailUrl ?? activeAttempt(slot)?.thumbnailUrl ?? null;
                                const isActiveVideo = activeVideoSlot === slot.sourceOrder;

                                if (!isActiveVideo) {
                                  return (
                                    <button
                                      className="video-preview-button"
                                      onClick={() => {
                                        void ensureVideoPlaybackSrc(slot)
                                          .then(() => setActiveVideoSlot(slot.sourceOrder))
                                          .catch(() => {
                                            if (useRemoteVideo) {
                                              setActiveVideoSlot(slot.sourceOrder);
                                              return;
                                            }
                                            markLocalAssetFailure(slot);
                                          });
                                      }}
                                      title={`Abrir preview do slot ${slot.sourceOrder}`}
                                    >
                                      {thumbnailSrc ? (
                                        <img
                                          src={thumbnailSrc}
                                          alt={`Thumbnail do slot ${slot.sourceOrder}`}
                                          loading="lazy"
                                          onError={() => markVideoThumbnailFailure(slot)}
                                        />
                                      ) : (
                                        <div className="video-preview-fallback">
                                          <strong>{String(slot.sourceOrder).padStart(2, "0")}</strong>
                                          <small>Vídeo pronto</small>
                                        </div>
                                      )}
                                      <span className="video-preview-overlay">
                                        <i>▶</i>
                                        <span>Assistir</span>
                                      </span>
                                    </button>
                                  );
                                }

                                if (!assetSrc) {
                                  return (
                                    <div className="asset-placeholder">
                                      <strong>{String(slot.sourceOrder).padStart(2, "0")}</strong>
                                      <small>Preview indisponível</small>
                                    </div>
                                  );
                                }

                                return (
                                  <video
                                    src={assetSrc}
                                    controls
                                    autoPlay
                                    preload="metadata"
                                    playsInline
                                    onError={() => {
                                      if (useRemoteVideo) {
                                        markRemoteAssetFailure(slot);
                                        return;
                                      }
                                      markLocalAssetFailure(slot);
                                    }}
                                  />
                                );
                              })() : (
                                <div className="asset-placeholder">
                                  <strong>{String(slot.sourceOrder).padStart(2, "0")}</strong>
                                  <small>{slotStatusLabel(slot.status)}</small>
                                </div>
                              )}
                              <div className="asset-actions">
                                <button
                                  className="asset-action"
                                  disabled={busy === `refresh-${slot.sourceOrder}`}
                                  onClick={() => handleRefreshSlotAsset(slot.sourceOrder)}
                                  title={`Sincronizar preview do slot ${slot.sourceOrder}`}
                                >
                                  {busy === `refresh-${slot.sourceOrder}` ? "..." : "👁"}
                                </button>
                                <button
                                  className="asset-action"
                                  disabled={
                                    busy === "generate"
                                    || busy === "animate"
                                    || busy.startsWith("animate-")
                                    || busy.startsWith("retry-")
                                    || (selectedGenerationProgress?.active ?? false)
                                  }
                                  onClick={() => handleRetrySlot(slot)}
                                  title={`Retentar geração do slot ${slot.sourceOrder}`}
                                >
                                  {busy === `retry-${slot.sourceOrder}` ? "..." : "↻"}
                                </button>
                              </div>
                              <button
                                className="asset-action asset-action-play"
                                disabled={
                                  !canPlaySlot(slot)
                                  || !selected?.flowProjectId
                                  || !bridgeStatus.extensionConnected
                                  || busy === "generate"
                                  || busy === "animate"
                                  || busy.startsWith("animate-")
                                  || busy.startsWith("retry-")
                                  || (selectedGenerationProgress?.active ?? false)
                                }
                                onClick={() => handleAnimateSlot(slot.sourceOrder)}
                                title={
                                  canAnimateSlot(slot)
                                    ? `Animar slot ${slot.sourceOrder}`
                                    : canPlaySlot(slot)
                                      ? `Reprocessar slot ${slot.sourceOrder} como imagem + animação`
                                      : "Slot não disponível para animação"
                                }
                              >
                                {busy === `animate-${slot.sourceOrder}` ? "..." : "▶"}
                              </button>
                              <span className={`asset-badge ${slot.currentFileType === "video" || slot.assetType === "video" ? "badge-video" : "badge-image"}`}>
                                {slot.currentFileType === "video" || slot.assetType === "video" ? "VID" : "IMG"}
                              </span>
                            </div>
                            <div className="asset-info">
                              <strong>Slot {String(slot.sourceOrder).padStart(2, "0")}</strong>
                              <small>{slotStatusLabel(slot.status)}</small>
                              <small>{remoteStatusLabel(slot.remoteStatus ?? activeAttempt(slot)?.remoteStatus)}</small>
                              <small>{(() => {
                                const currentAttempt = activeAttempt(slot);
                                return currentAttempt
                                  ? `Tentativa ${currentAttempt.attemptNumber}${currentAttempt.workflowId ? ` · WF ${currentAttempt.workflowId.slice(0, 8)}` : ""}`
                                  : `ID ${(slot.slotId ?? `slot_${String(slot.sourceOrder).padStart(4, "0")}`).slice(-8)}`;
                              })()}</small>
                              <div className="asset-id-list">
                                {slot.mediaId && <code title={slot.mediaId}>MID {slot.mediaId.slice(-12)}</code>}
                                {slot.imageMediaId && slot.imageMediaId !== slot.mediaId && (
                                  <code title={slot.imageMediaId}>IMG {slot.imageMediaId.slice(-12)}</code>
                                )}
                                {slot.operationId && <code title={slot.operationId}>OP {slot.operationId.slice(-12)}</code>}
                              </div>
                              <p>{slot.prompt}</p>
                              {slot.remainingCredits != null && <small>Creditos restantes: {slot.remainingCredits}</small>}
                              {slot.error && <small className="asset-error">{slot.error}</small>}
                            </div>
                          </div>
                        ))}
                      </div>
                    </section>
                  )}
                </>
              )}
            </section>
          </div>}

          {activeSection === "sincronizacao" && (
            <SynchronizationView
              projects={projects}
              selected={selected}
              onSelect={setSelectedId}
              busy={busy === "audio"}
              pendingAudioPath={selected ? (pendingAudioByProject[selected.localProjectId] ?? null) : null}
              assetSrtMode={assetSrtMode}
              onAssetSrtModeChange={setAssetSrtMode}
              assetSrtValue={assetSrtValue}
              onAssetSrtValueChange={setAssetSrtValue}
              transitionMode={transitionMode}
              onTransitionModeChange={setTransitionMode}
              onChooseAudio={handleChooseAudio}
              onProcessAudio={handleAudio}
              onDownload={handleDownloadSrt}
              onNavigate={setActiveSection}
            />
          )}

          {activeSection === "sessoes" && (
            <SessionsView
              bridge={bridgeStatus}
              assembly={assemblyStatus}
              busy={busy === "browser"}
              assemblyBusy={busy === "assembly"}
              onOpenBrowser={handleOpenFlowBrowser}
              onRefresh={refreshBridge}
              onSaveAssemblyKeys={handleSaveAssemblyKeys}
              onClearAssemblyKeys={handleClearAssemblyKeys}
            />
          )}
        </main>
      </div>

      {createOpen && (
        <div className="dialog-backdrop" onMouseDown={() => setCreateOpen(false)}>
          <div className="dialog-panel create-production-dialog" onMouseDown={(event) => event.stopPropagation()}>
            <div className="dialog-head">
              <div><span className="section-kicker">NOVA PRODUÇÃO</span><h2>Preparar pasta de trabalho</h2></div>
              <button className="icon-button" onClick={() => setCreateOpen(false)}>×</button>
            </div>
            <label>Nome da produção<input autoFocus value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Ex.: Documentário Aurora" /></label>
            <p className="dialog-note">O projeto correspondente será criado no Flow e vinculado automaticamente quando a ponte estiver conectada.</p>
            <div className="folder-preview"><span>PASTA FINAL DOS ASSETS</span><code>{assetOutputDir || "Usar downloads dentro da pasta local da produção"}</code></div>
            <div className="dialog-actions">
              <button className="quiet-button" onClick={handleChooseAssetOutputDir}>Escolher pasta final</button>
            </div>
            <div className="folder-preview"><span>PASTA LOCAL</span><code>Documentos\FlowContent Auto\{title ? title.toLowerCase().replace(/\s+/g, "-") : "nome-da-producao"}</code></div>
            <div className="dialog-actions">
              <button className="quiet-button" onClick={() => setCreateOpen(false)}>Cancelar</button>
              <button className="secondary-button" disabled={!title.trim() || busy === "create"} onClick={handleCreate}>{busy === "create" ? "Criando..." : "Criar produção"}</button>
            </div>
          </div>
        </div>
      )}
      {deleteTarget && (
        <div className="dialog-backdrop" onMouseDown={() => busy !== "delete" && setDeleteTarget(null)}>
          <div className="dialog-panel delete-production-dialog" onMouseDown={(event) => event.stopPropagation()}>
            <div className="dialog-head">
              <div><span className="section-kicker danger-kicker">EXCLUIR PRODUÇÃO</span><h2>{deleteTarget.title}</h2></div>
              <button className="icon-button" disabled={busy === "delete"} onClick={() => setDeleteTarget(null)}>×</button>
            </div>
            <div className="delete-warning">
              <strong>Esta ação remove a produção local por completo.</strong>
              <p>Áudio, SRTs, prompts e metadados serão apagados. O projeto remoto existente no Flow não será excluído.</p>
              <code>{deleteTarget.projectRoot}</code>
            </div>
            <div className="dialog-actions">
              <button className="quiet-button" disabled={busy === "delete"} onClick={() => setDeleteTarget(null)}>Cancelar</button>
              <button className="danger-button solid" disabled={busy === "delete"} onClick={handleDelete}>
                {busy === "delete" ? "Excluindo..." : "Excluir arquivos locais"}
              </button>
            </div>
          </div>
        </div>
      )}
      <div className={`toast ${toast ? "visible" : ""}`} role="status" aria-live="polite">{toast}</div>
    </>
  );
}
