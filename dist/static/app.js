(function () {
    const state = {
        page: 1,
        size: 10,
        selectedId: null,
        isBusy: false,
        hasNext: false,
        month: "",
    };

    const el = {
        date: document.getElementById("journal-date"),
        content: document.getElementById("journal-content"),
        createBtn: document.getElementById("create-btn"),
        updateBtn: document.getElementById("update-btn"),
        deleteBtn: document.getElementById("delete-btn"),
        syncOnSave: document.getElementById("sync-on-save"),
        newBtn: document.getElementById("new-btn"),
        syncBtn: document.getElementById("sync-btn"),
        uploadInput: document.getElementById("upload-input"),
        uploadBtn: document.getElementById("upload-btn"),
        importZipInput: document.getElementById("import-zip-input"),
        importPatternPreset: document.getElementById("import-pattern-preset"),
        importPatterns: document.getElementById("import-patterns"),
        syncOutputPath: document.getElementById("sync-output-path"),
        syncCommitMessage: document.getElementById("sync-commit-message"),
        phYyyy: document.getElementById("ph-yyyy"),
        phMm: document.getElementById("ph-mm"),
        phM: document.getElementById("ph-m"),
        phDd: document.getElementById("ph-dd"),
        phD: document.getElementById("ph-d"),
        phDate: document.getElementById("ph-date"),
        phTimestamp: document.getElementById("ph-timestamp"),
        phCount: document.getElementById("ph-count"),
        loadSettingsBtn: document.getElementById("load-settings-btn"),
        saveSettingsBtn: document.getElementById("save-settings-btn"),
        importPatternHelp: document.getElementById("import-pattern-help"),
        importResult: document.getElementById("import-result"),
        importBtn: document.getElementById("import-btn"),
        status: document.getElementById("status"),
        filterDate: document.getElementById("filter-date"),
        pageSize: document.getElementById("page-size"),
        searchBtn: document.getElementById("search-btn"),
        list: document.getElementById("journal-list"),
        prevBtn: document.getElementById("prev-btn"),
        nextBtn: document.getElementById("next-btn"),
        pageLabel: document.getElementById("page-label"),
        preview: document.getElementById("preview"),
        monthInput: document.getElementById("month-input"),
        monthSearchBtn: document.getElementById("month-search-btn"),
        monthGrid: document.getElementById("month-grid"),
    };

    const DEFAULT_DATE_PLACEHOLDERS = {
        yyyy: "{yyyy}",
        mm: "{MM}",
        m: "{M}",
        dd: "{dd}",
        d: "{d}",
        date: "{date}",
        timestamp: "{timestamp}",
        count: "{count}",
    };

    function today() {
        const now = new Date();
        const m = String(now.getMonth() + 1).padStart(2, "0");
        const d = String(now.getDate()).padStart(2, "0");
        return `${now.getFullYear()}-${m}-${d}`;
    }

    function currentMonth() {
        return today().slice(0, 7);
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
        el.importBtn.disabled = state.isBusy;
        el.searchBtn.disabled = state.isBusy;
        el.loadSettingsBtn.disabled = state.isBusy;
        el.saveSettingsBtn.disabled = state.isBusy;
        el.monthSearchBtn.disabled = state.isBusy;
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
        clearActiveInMonth();
        refreshButtons();
    }

    function clearActiveInList() {
        const active = el.list.querySelector(".journal-item.active");
        if (active) active.classList.remove("active");
    }

    function clearActiveInMonth() {
        const active = el.monthGrid.querySelector(".month-day.active");
        if (active) active.classList.remove("active");
    }

    function findMonthDayButton(date) {
        return el.monthGrid.querySelector(`.month-day[data-date="${date}"]`);
    }

    function findListItemButton(date) {
        return el.list.querySelector(`.journal-item[data-date="${date}"]`);
    }

    function selectJournal(item, btnRef, source) {
        state.selectedId = item.id;
        el.date.value = item.date;
        el.content.value = item.content;
        renderPreview();
        clearActiveInList();
        clearActiveInMonth();
        if (btnRef) btnRef.classList.add("active");
        if (source !== "month") {
            const dayBtn = findMonthDayButton(item.date);
            if (dayBtn) dayBtn.classList.add("active");
        }
        if (source !== "list") {
            const listBtn = findListItemButton(item.date);
            if (listBtn) listBtn.classList.add("active");
        }
        refreshButtons();
    }

    function selectDateForCreate(date, btnRef) {
        state.selectedId = null;
        el.date.value = date;
        el.content.value = "";
        renderPreview();
        clearActiveInList();
        clearActiveInMonth();
        if (btnRef) btnRef.classList.add("active");
        refreshButtons();
        setStatus(`已选择 ${date}，可直接输入并点击“创建”`, true);
    }

    function listItemView(item) {
        const li = document.createElement("li");
        const btn = document.createElement("button");
        btn.className = "journal-item";
        btn.type = "button";
        btn.dataset.date = item.date;

        const snippet = (item.content || "").replace(/\n+/g, " ").slice(0, 80);
        btn.innerHTML = `
      <div class="item-date">#${item.id} · ${item.date}</div>
      <div class="item-content">${escapeHtml(snippet || "(无内容)")}</div>
    `;
        btn.addEventListener("click", () => selectJournal(item, btn, "list"));
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
        if (state.selectedId) {
            const selected = items.find((it) => it.id === state.selectedId);
            if (selected) {
                const btn = findListItemButton(selected.date);
                if (btn) btn.classList.add("active");
            }
        }
        state.hasNext = items.length >= state.size;
        refreshButtons();
    }

    function parseMonthParts(month) {
        const parts = (month || "").split("-");
        if (parts.length !== 2) return null;
        const year = Number(parts[0]);
        const m = Number(parts[1]);
        if (!Number.isInteger(year) || !Number.isInteger(m) || m < 1 || m > 12) return null;
        return {year, month: m};
    }

    function renderMonthGrid(month, monthItems) {
        el.monthGrid.innerHTML = "";
        const parsed = parseMonthParts(month);
        if (!parsed) return;
        const {year, month: m} = parsed;
        const firstWeekday = (new Date(year, m - 1, 1).getDay() + 6) % 7;
        const daysInMonth = new Date(year, m, 0).getDate();
        const itemMap = new Map((monthItems || []).map((it) => [it.date, it]));
        const todayStr = today();

        for (let i = 0; i < firstWeekday; i += 1) {
            const blank = document.createElement("div");
            blank.className = "month-day blank";
            el.monthGrid.appendChild(blank);
        }

        for (let day = 1; day <= daysInMonth; day += 1) {
            const date = `${year}-${String(m).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
            const item = itemMap.get(date);
            const btn = document.createElement("button");
            btn.type = "button";
            btn.className = `month-day ${item ? "has-entry" : "empty"}`;
            btn.dataset.date = date;
            btn.innerHTML = `<span class="n">${day}</span><span class="s">${item ? "已写" : "新增"}</span>`;
            if (date === todayStr) btn.classList.add("today");
            if (state.selectedId && item && item.id === state.selectedId) btn.classList.add("active");

            btn.addEventListener("click", () => {
                if (item) {
                    selectJournal(item, btn, "month");
                } else {
                    selectDateForCreate(date, btn);
                }
            });
            el.monthGrid.appendChild(btn);
        }
    }

    async function loadMonthView(silent = false, withBusy = true) {
        if (!state.month) state.month = currentMonth();
        if (withBusy) setBusy(true);
        try {
            const data = await request(`/journal?date=${encodeURIComponent(state.month)}&page=1&size=100`);
            const items = data.data || [];
            renderMonthGrid(state.month, items);
            if (!silent) setStatus(`月份 ${state.month} 已加载，共 ${items.length} 条`, true);
        } catch (err) {
            setStatus(`月份加载失败: ${err.message}`, false);
        } finally {
            if (withBusy) setBusy(false);
        }
    }

    function buildQuery() {
        const params = new URLSearchParams();
        params.set("page", String(state.page));
        params.set("size", String(state.size));
        if (el.filterDate.value) params.set("date", el.filterDate.value);
        return params.toString();
    }

    async function loadList(silent = false) {
        setBusy(true);
        try {
            const data = await request(`/journal?${buildQuery()}`);
            renderList(data.data || []);
            el.pageLabel.textContent = `第 ${state.page} 页`;
            if (!silent) setStatus("列表已刷新", true);
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
                auto_sync: el.syncOnSave.checked,
            };
            const resp = await request("/journal", {
                method: "POST",
                headers: {"Content-Type": "application/json"},
                body: JSON.stringify(payload),
            });
            let msg = `保存成功: #${resp.data.id}`;
            if (el.syncOnSave.checked) {
                const syncMsg = await syncJournalInternal();
                msg += `，${syncMsg}`;
            }
            setStatus(msg, true);
            state.page = 1;
            await loadList(true);
            await loadMonthView(true, false);
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
                auto_sync: el.syncOnSave.checked,
            };
            await request(`/journal/${state.selectedId}`, {
                method: "PUT",
                headers: {"Content-Type": "application/json"},
                body: JSON.stringify(payload),
            });
            let msg = `更新成功: #${state.selectedId}`;
            if (el.syncOnSave.checked) {
                const syncMsg = await syncJournalInternal();
                msg += `，${syncMsg}`;
            }
            setStatus(msg, true);
            await loadList(true);
            await loadMonthView(true, false);
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
            await loadMonthView(true, false);
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

    function parsePatternLines(text) {
        return (text || "")
            .split(/\r?\n|,|;/)
            .map((v) => v.trim())
            .filter((v) => v.length > 0);
    }

    function uniqueStrings(arr) {
        return [...new Set(arr)];
    }

    function getDatePlaceholdersFromForm() {
        return {
            yyyy: (el.phYyyy.value || "").trim(),
            mm: (el.phMm.value || "").trim(),
            m: (el.phM.value || "").trim(),
            dd: (el.phDd.value || "").trim(),
            d: (el.phD.value || "").trim(),
            date: (el.phDate.value || "").trim(),
            timestamp: (el.phTimestamp.value || "").trim(),
            count: (el.phCount.value || "").trim(),
        };
    }

    function normalizeDatePlaceholders(input) {
        const fallback = DEFAULT_DATE_PLACEHOLDERS;
        const v = input || {};
        const out = {
            yyyy: (v.yyyy || fallback.yyyy).trim(),
            mm: (v.mm || fallback.mm).trim(),
            m: (v.m || fallback.m).trim(),
            dd: (v.dd || fallback.dd).trim(),
            d: (v.d || fallback.d).trim(),
            date: (v.date || fallback.date).trim(),
            timestamp: (v.timestamp || fallback.timestamp).trim(),
            count: (v.count || fallback.count).trim(),
        };
        const fields = Object.entries(out);
        for (const [key, token] of fields) {
            if (!token) throw new Error(`占位符 ${key} 不能为空`);
            if (!(token.startsWith("{") && token.endsWith("}") && token.length >= 3)) {
                throw new Error(`占位符 ${key} 必须是 {xxx} 格式`);
            }
        }
        const uniq = new Set(fields.map(([, token]) => token));
        if (uniq.size !== fields.length) {
            throw new Error("占位符不能重复");
        }
        return out;
    }

    function setDatePlaceholdersToForm(placeholders) {
        el.phYyyy.value = placeholders.yyyy;
        el.phMm.value = placeholders.mm;
        el.phM.value = placeholders.m;
        el.phDd.value = placeholders.dd;
        el.phD.value = placeholders.d;
        el.phDate.value = placeholders.date;
        el.phTimestamp.value = placeholders.timestamp;
        el.phCount.value = placeholders.count;
    }

    function buildImportPresets(placeholders) {
        return {
            default: [
                `${placeholders.yyyy}/${placeholders.mm}/${placeholders.dd}.md`,
                `${placeholders.yyyy}/${placeholders.mm}-${placeholders.dd}.md`,
                `${placeholders.yyyy}-${placeholders.mm}-${placeholders.dd}.md`,
                `${placeholders.yyyy}_${placeholders.mm}_${placeholders.dd}.md`,
            ],
            ymd: [`${placeholders.yyyy}/${placeholders.mm}/${placeholders.dd}.md`],
            ymd_dash: [`${placeholders.yyyy}/${placeholders.mm}-${placeholders.dd}.md`],
            year_mix: [`${placeholders.yyyy}/${placeholders.yyyy}_${placeholders.mm}/${placeholders.d}.md`],
            custom: [],
        };
    }

    function refreshPresetLabels(placeholders) {
        const map = buildImportPresets(placeholders);
        const options = el.importPatternPreset.querySelectorAll("option");
        options.forEach((opt) => {
            if (opt.value === "ymd") opt.textContent = map.ymd[0];
            if (opt.value === "ymd_dash") opt.textContent = map.ymd_dash[0];
            if (opt.value === "year_mix") opt.textContent = map.year_mix[0];
        });
    }

    function resolveImportPatterns() {
        const preset = el.importPatternPreset.value || "default";
        const placeholders = normalizeDatePlaceholders(getDatePlaceholdersFromForm());
        const importPresets = buildImportPresets(placeholders);
        const fromInput = parsePatternLines(el.importPatterns.value);
        if (fromInput.length > 0) {
            return uniqueStrings(fromInput);
        }
        return uniqueStrings(importPresets[preset] || importPresets.default);
    }

    function fillPresetPatterns() {
        const preset = el.importPatternPreset.value || "default";
        const placeholders = normalizeDatePlaceholders(getDatePlaceholdersFromForm());
        const importPresets = buildImportPresets(placeholders);
        const list = importPresets[preset] || [];
        el.importPatterns.value = list.join("\n");
    }

    function patternToExample(pattern, placeholders) {
        return pattern
            .replaceAll(placeholders.yyyy, "2026")
            .replaceAll(placeholders.mm, "01")
            .replaceAll(placeholders.m, "1")
            .replaceAll(placeholders.dd, "02")
            .replaceAll(placeholders.d, "2")
            .replaceAll(placeholders.date, "2026-01-02");
    }

    function renderPatternHelp() {
        try {
            const placeholders = normalizeDatePlaceholders(getDatePlaceholdersFromForm());
            refreshPresetLabels(placeholders);
            const patterns = resolveImportPatterns();
            const examples = patterns.map((p) => `- ${p}  ->  ${patternToExample(p, placeholders)}`);
            const text = [
                `可解析占位符: ${placeholders.yyyy} ${placeholders.mm}/${placeholders.m} ${placeholders.dd}/${placeholders.d} ${placeholders.date}`,
                "{date} 支持: yyyy-MM-dd, yyyy_MM_dd, yyyy.MM.dd, yyyyMMdd, yyyy-M-d",
                `Git Commit占位符: ${placeholders.yyyy} ${placeholders.mm} ${placeholders.m} ${placeholders.dd} ${placeholders.d} ${placeholders.date} ${placeholders.timestamp} ${placeholders.count} {journal_dd} {journal_d}`,
                "当前模板示例:",
                ...examples,
            ].join("\n");
            el.importPatternHelp.textContent = text;
        } catch (err) {
            el.importPatternHelp.textContent = `占位符配置错误: ${err.message}`;
        }
    }

    async function loadPersistedSettings() {
        const resp = await request("/settings", {method: "GET"});
        const data = resp.data || {};
        const datePlaceholders = normalizeDatePlaceholders(data.datePlaceholders || DEFAULT_DATE_PLACEHOLDERS);
        setDatePlaceholdersToForm(datePlaceholders);
        const patterns = data.importPatterns || [];
        if (patterns.length) {
            el.importPatternPreset.value = "custom";
            el.importPatterns.value = patterns.join("\n");
        }
        if (data.syncOutputPath) {
            el.syncOutputPath.value = data.syncOutputPath;
        }
        if (data.syncCommitMessage) {
            el.syncCommitMessage.value = data.syncCommitMessage;
        }
        renderPatternHelp();
    }

    async function savePersistedSettings() {
        const datePlaceholders = normalizeDatePlaceholders(getDatePlaceholdersFromForm());
        const payload = {
            importPatterns: resolveImportPatterns(),
            syncOutputPath: (el.syncOutputPath.value || "").trim(),
            syncCommitMessage: (el.syncCommitMessage.value || "").trim(),
            datePlaceholders,
        };
        if (!payload.syncOutputPath) {
            throw new Error("请填写 Git Markdown 输出路径模板");
        }
        if (!payload.syncCommitMessage) {
            throw new Error("请填写 Git Commit 模板");
        }
        await request("/settings", {
            method: "PUT",
            headers: {"Content-Type": "application/json"},
            body: JSON.stringify(payload),
        });
    }

    function renderImportResult(result, errorMsg) {
        if (errorMsg) {
            el.importResult.textContent = `导入失败\n${errorMsg}`;
            return;
        }
        if (!result) {
            el.importResult.textContent = "等待导入...";
            return;
        }

        const lines = [
            `totalMarkdownFiles: ${result.totalMarkdownFiles}`,
            `matchedFiles: ${result.matchedFiles}`,
            `importedCount: ${result.importedCount}`,
            `skippedCount: ${result.skippedCount}`,
            "skippedDetails:",
        ];

        const details = result.skippedDetails || [];
        if (details.length) {
            details.forEach((item) => {
                lines.push(`- path: ${item.path}`);
                lines.push(`  reason: ${item.reason}`);
            });
        } else if ((result.skippedPaths || []).length) {
            (result.skippedPaths || []).forEach((item) => {
                lines.push(`- ${item}`);
            });
        } else {
            lines.push("- (none)");
        }
        el.importResult.textContent = lines.join("\n");
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

    async function importZipJournals() {
        const zip = el.importZipInput.files && el.importZipInput.files[0];
        if (!zip) {
            setStatus("请先选择 zip 压缩包", false);
            return;
        }
        const patterns = resolveImportPatterns();
        if (!patterns.length) {
            setStatus("请至少提供一个路径模板", false);
            return;
        }

        setBusy(true);
        try {
            const fd = new FormData();
            fd.append("file", zip, zip.name);
            fd.append("patterns", JSON.stringify(patterns));
            const resp = await request("/journal/import/zip", {
                method: "POST",
                body: fd,
            });
            const result = resp.data;
            setStatus(
                `导入完成：匹配 ${result.matchedFiles}，成功 ${result.importedCount}，跳过 ${result.skippedCount}`,
                true
            );
            renderImportResult(result);
            await loadList();
            await loadMonthView(true, false);
        } catch (err) {
            setStatus(`导入失败: ${err.message}`, false);
            renderImportResult(null, err.message);
        } finally {
            setBusy(false);
        }
    }

    async function syncJournalInternal() {
        const resp = await request("/sync/journal", {method: "POST"});
        const result = resp.data;
        return `同步完成: ${result.message}${result.commitId ? ` (${result.commitId.slice(0, 8)})` : ""}`;
    }

    async function syncJournal() {
        setBusy(true);
        try {
            const msg = await syncJournalInternal();
            setStatus(msg, true);
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
    el.importBtn.addEventListener("click", importZipJournals);
    el.importPatternPreset.addEventListener("change", () => {
        fillPresetPatterns();
        renderPatternHelp();
    });
    el.importPatterns.addEventListener("input", renderPatternHelp);
    [el.phYyyy, el.phMm, el.phM, el.phDd, el.phD, el.phDate, el.phTimestamp, el.phCount].forEach((node) => {
        node.addEventListener("input", renderPatternHelp);
    });
    el.loadSettingsBtn.addEventListener("click", async () => {
        setBusy(true);
        try {
            await loadPersistedSettings();
            setStatus("已加载持久化配置", true);
        } catch (err) {
            setStatus(`加载配置失败: ${err.message}`, false);
        } finally {
            setBusy(false);
        }
    });
    el.saveSettingsBtn.addEventListener("click", async () => {
        setBusy(true);
        try {
            await savePersistedSettings();
            setStatus("已保存持久化配置", true);
        } catch (err) {
            setStatus(`保存配置失败: ${err.message}`, false);
        } finally {
            setBusy(false);
        }
    });
    el.syncBtn.addEventListener("click", syncJournal);
    el.monthSearchBtn.addEventListener("click", () => {
        state.month = el.monthInput.value || currentMonth();
        loadMonthView();
    });
    el.monthInput.addEventListener("change", () => {
        state.month = el.monthInput.value || currentMonth();
        loadMonthView(true, false);
    });

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
        setDatePlaceholdersToForm(DEFAULT_DATE_PLACEHOLDERS);
        el.date.value = today();
        state.month = currentMonth();
        el.monthInput.value = state.month;
        fillPresetPatterns();
        el.syncOutputPath.value = `journals/${DEFAULT_DATE_PLACEHOLDERS.yyyy}/${DEFAULT_DATE_PLACEHOLDERS.mm}-${DEFAULT_DATE_PLACEHOLDERS.dd}/${DEFAULT_DATE_PLACEHOLDERS.d}.md`;
        el.syncCommitMessage.value = `sync ${DEFAULT_DATE_PLACEHOLDERS.date} jd={journal_dd}/{journal_d} count=${DEFAULT_DATE_PLACEHOLDERS.count} ts=${DEFAULT_DATE_PLACEHOLDERS.timestamp}`;
        renderPatternHelp();
        renderImportResult(null);
        renderPreview();
        refreshButtons();
        loadPersistedSettings().catch(() => {
        });
        loadList();
        loadMonthView(true, false);
    }

    init();
})();
