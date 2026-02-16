(function () {
    const state = {
        page: 1,
        size: 10,
        selectedId: null,
        isBusy: false,
        hasNext: false,
    };

    const el = {
        date: document.getElementById("journal-date"),
        content: document.getElementById("journal-content"),
        createBtn: document.getElementById("create-btn"),
        updateBtn: document.getElementById("update-btn"),
        deleteBtn: document.getElementById("delete-btn"),
        newBtn: document.getElementById("new-btn"),
        syncBtn: document.getElementById("sync-btn"),
        uploadInput: document.getElementById("upload-input"),
        uploadBtn: document.getElementById("upload-btn"),
        status: document.getElementById("status"),
        filterDate: document.getElementById("filter-date"),
        pageSize: document.getElementById("page-size"),
        searchBtn: document.getElementById("search-btn"),
        list: document.getElementById("journal-list"),
        prevBtn: document.getElementById("prev-btn"),
        nextBtn: document.getElementById("next-btn"),
        pageLabel: document.getElementById("page-label"),
        preview: document.getElementById("preview"),
    };

    function today() {
        const now = new Date();
        const m = String(now.getMonth() + 1).padStart(2, "0");
        const d = String(now.getDate()).padStart(2, "0");
        return `${now.getFullYear()}-${m}-${d}`;
    }

    function setStatus(text, ok) {
        el.status.textContent = text;
        el.status.className = `status show ${ok ? "ok" : "err"}`;
    }

    function clearStatus() {
        el.status.className = "status";
        el.status.textContent = "";
    }

    function refreshButtons() {
        const hasSelection = !!state.selectedId;
        el.createBtn.disabled = state.isBusy;
        el.newBtn.disabled = state.isBusy;
        el.syncBtn.disabled = state.isBusy;
        el.uploadBtn.disabled = state.isBusy;
        el.searchBtn.disabled = state.isBusy;
        el.prevBtn.disabled = state.isBusy || state.page <= 1;
        el.nextBtn.disabled = state.isBusy || !state.hasNext;
        el.updateBtn.disabled = state.isBusy || !hasSelection;
        el.deleteBtn.disabled = state.isBusy || !hasSelection;
    }

    function setBusy(busy) {
        state.isBusy = busy;
        refreshButtons();
    }

    async function request(url, options) {
        const resp = await fetch(url, options);
        const body = await resp.json();
        if (!resp.ok || body.code !== 200) {
            throw new Error(body.msg || `request failed: ${url}`);
        }
        return body;
    }

    function escapeHtml(s) {
        return s
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/\"/g, "&quot;")
            .replace(/'/g, "&#39;");
    }

    function mdToHtml(md) {
        function isImageUrl(url) {
            return /(^\/files\/picture\/)|\.(png|jpe?g|gif|webp|svg|bmp)$/i.test(url || "");
        }

        function isVideoUrl(url) {
            return /(^\/files\/media\/)|\.(mp4|webm|mov|m4v|ogg)$/i.test(url || "");
        }

        function isAudioUrl(url) {
            return /\.(mp3|wav|m4a|aac|flac|oga)$/i.test(url || "");
        }

        let html = escapeHtml(md || "");
        html = html.replace(/^###\s+(.*)$/gm, "<h3>$1</h3>");
        html = html.replace(/^##\s+(.*)$/gm, "<h2>$1</h2>");
        html = html.replace(/^#\s+(.*)$/gm, "<h1>$1</h1>");
        html = html.replace(/\*\*(.*?)\*\*/g, "<strong>$1</strong>");
        html = html.replace(/\*(.*?)\*/g, "<em>$1</em>");
        html = html.replace(/`([^`]+)`/g, "<code>$1</code>");
        html = html.replace(/!\[(.*?)\]\((.*?)\)/g, (m, alt, src) => {
            if (isVideoUrl(src)) return `<video src="${src}" controls></video>`;
            if (isAudioUrl(src)) return `<audio src="${src}" controls></audio>`;
            return `<img alt="${alt}" src="${src}">`;
        });
        html = html.replace(/\[(.*?)\]\((.*?)\)/g, (m, text, href) => {
            if (isImageUrl(href)) return `<img alt="${text}" src="${href}">`;
            if (isVideoUrl(href)) return `<video src="${href}" controls></video>`;
            if (isAudioUrl(href)) return `<audio src="${href}" controls></audio>`;
            return `<a href="${href}" target="_blank" rel="noopener noreferrer">${text}</a>`;
        });

        const lines = html.split("\n");
        let inCode = false;
        const out = [];
        for (const line of lines) {
            if (line.trim().startsWith("```")) {
                out.push(inCode ? "</pre>" : "<pre>");
                inCode = !inCode;
                continue;
            }
            if (inCode) {
                out.push(line);
            } else if (!/^<h[1-3]>/.test(line) && !/^<pre>/.test(line) && line.trim() !== "") {
                out.push(`<p>${line}</p>`);
            } else {
                out.push(line);
            }
        }

        return out.join("\n");
    }

    function renderPreview() {
        el.preview.innerHTML = mdToHtml(el.content.value);
    }

    function resetEditor() {
        state.selectedId = null;
        el.date.value = today();
        el.content.value = "";
        renderPreview();
        clearActiveInList();
        refreshButtons();
    }

    function clearActiveInList() {
        const active = el.list.querySelector(".journal-item.active");
        if (active) active.classList.remove("active");
    }

    function selectJournal(item, btnRef) {
        state.selectedId = item.id;
        el.date.value = item.date;
        el.content.value = item.content;
        renderPreview();
        clearActiveInList();
        btnRef.classList.add("active");
        refreshButtons();
    }

    function listItemView(item) {
        const li = document.createElement("li");
        const btn = document.createElement("button");
        btn.className = "journal-item";
        btn.type = "button";

        const snippet = (item.content || "").replace(/\n+/g, " ").slice(0, 80);
        btn.innerHTML = `
      <div class="item-date">#${item.id} · ${item.date}</div>
      <div class="item-content">${escapeHtml(snippet || "(无内容)")}</div>
    `;
        btn.addEventListener("click", () => selectJournal(item, btn));
        li.appendChild(btn);
        return li;
    }

    function renderList(items) {
        el.list.innerHTML = "";
        if (!items.length) {
            const li = document.createElement("li");
            li.innerHTML = '<button class="journal-item" type="button" disabled>当前条件下没有日记</button>';
            el.list.appendChild(li);
            state.hasNext = false;
            refreshButtons();
            return;
        }
        items.forEach((it) => el.list.appendChild(listItemView(it)));
        state.hasNext = items.length >= state.size;
        refreshButtons();
    }

    function buildQuery() {
        const params = new URLSearchParams();
        params.set("page", String(state.page));
        params.set("size", String(state.size));
        if (el.filterDate.value) params.set("date", el.filterDate.value);
        return params.toString();
    }

    async function loadList() {
        setBusy(true);
        try {
            const data = await request(`/journal?${buildQuery()}`);
            renderList(data.data || []);
            el.pageLabel.textContent = `第 ${state.page} 页`;
            setStatus("列表已刷新", true);
        } catch (err) {
            setStatus(`加载失败: ${err.message}`, false);
        } finally {
            setBusy(false);
        }
    }

    async function createJournal() {
        if (!el.date.value || !el.content.value.trim()) {
            setStatus("请填写日期和内容", false);
            return;
        }
        setBusy(true);
        try {
            const payload = {
                date: el.date.value,
                content: el.content.value,
            };
            const resp = await request("/journal", {
                method: "POST",
                headers: {"Content-Type": "application/json"},
                body: JSON.stringify(payload),
            });
            setStatus(`创建成功: #${resp.data.id}`, true);
            state.page = 1;
            await loadList();
        } catch (err) {
            setStatus(`创建失败: ${err.message}`, false);
        } finally {
            setBusy(false);
        }
    }

    async function updateJournal() {
        if (!state.selectedId) {
            setStatus("请先从右侧选择一条日记", false);
            return;
        }
        setBusy(true);
        try {
            const payload = {
                date: el.date.value,
                content: el.content.value,
            };
            await request(`/journal/${state.selectedId}`, {
                method: "PUT",
                headers: {"Content-Type": "application/json"},
                body: JSON.stringify(payload),
            });
            setStatus(`更新成功: #${state.selectedId}`, true);
            await loadList();
        } catch (err) {
            setStatus(`更新失败: ${err.message}`, false);
        } finally {
            setBusy(false);
        }
    }

    async function deleteJournal() {
        if (!state.selectedId) {
            setStatus("请先选择一条日记", false);
            return;
        }
        const ok = confirm(`确认删除 #${state.selectedId} ?`);
        if (!ok) return;

        setBusy(true);
        try {
            await request(`/journal/${state.selectedId}`, {method: "DELETE"});
            setStatus(`删除成功: #${state.selectedId}`, true);
            resetEditor();
            await loadList();
        } catch (err) {
            setStatus(`删除失败: ${err.message}`, false);
        } finally {
            setBusy(false);
        }
    }

    function markdownForUpload(name, uri, mime) {
        if ((mime || "").startsWith("image/")) {
            return `![${name}](${uri})`;
        }
        return `[${name}](${uri})`;
    }

    async function uploadAndInsert() {
        const file = el.uploadInput.files && el.uploadInput.files[0];
        if (!file) {
            setStatus("请先选择要上传的文件", false);
            return;
        }

        setBusy(true);
        try {
            const fd = new FormData();
            fd.append("file", file, file.name);
            const resp = await request("/upload", {
                method: "POST",
                body: fd,
            });
            const uri = resp.data;
            const line = markdownForUpload(file.name, uri, file.type);
            if (el.content.value && !el.content.value.endsWith("\n")) {
                el.content.value += "\n";
            }
            el.content.value += `${line}\n`;
            renderPreview();
            setStatus(`上传成功并已插入: ${uri}`, true);
        } catch (err) {
            setStatus(`上传失败: ${err.message}`, false);
        } finally {
            setBusy(false);
        }
    }

    async function syncJournal() {
        setBusy(true);
        try {
            const resp = await request("/sync/journal", {method: "POST"});
            const result = resp.data;
            setStatus(`同步完成: ${result.message}${result.commitId ? ` (${result.commitId.slice(0, 8)})` : ""}`, true);
        } catch (err) {
            setStatus(`同步失败: ${err.message}`, false);
        } finally {
            setBusy(false);
        }
    }

    el.content.addEventListener("input", renderPreview);
    el.createBtn.addEventListener("click", createJournal);
    el.updateBtn.addEventListener("click", updateJournal);
    el.deleteBtn.addEventListener("click", deleteJournal);
    el.newBtn.addEventListener("click", () => {
        clearStatus();
        resetEditor();
    });
    el.uploadBtn.addEventListener("click", uploadAndInsert);
    el.syncBtn.addEventListener("click", syncJournal);

    el.searchBtn.addEventListener("click", () => {
        state.page = 1;
        state.size = Number(el.pageSize.value) || 10;
        loadList();
    });

    el.prevBtn.addEventListener("click", () => {
        if (state.page <= 1) return;
        state.page -= 1;
        loadList();
    });

    el.nextBtn.addEventListener("click", () => {
        if (!state.hasNext) {
            setStatus("当前页可能已是最后一页", false);
            return;
        }
        state.page += 1;
        loadList();
    });

    el.pageSize.addEventListener("change", () => {
        state.size = Number(el.pageSize.value) || 10;
    });

    function init() {
        el.date.value = today();
        renderPreview();
        refreshButtons();
        loadList();
    }

    init();
})();
