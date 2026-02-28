/**
 * ITP Event Emitter — builds ITP events from DOM-extracted data,
 * applies privacy levels, and handles local storage fallback.
 */

const DB_NAME = "ghost_itp";
const DB_VERSION = 1;
const STORE_NAME = "events";

export class ITPEmitter {
  constructor() {
    this._db = null;
  }

  /**
   * Apply privacy level to an ITP event.
   * - Minimal: hash all content fields with SHA-256
   * - Standard: hash message content, keep metadata
   * - Full: keep everything in plaintext
   * - Research: keep everything + add extra metadata
   */
  applyPrivacy(event, level) {
    if (level === "full" || level === "research") {
      return event;
    }

    const processed = { ...event };

    if (level === "minimal") {
      // Hash all content fields
      if (processed.content) {
        processed.content = this._hashPlaceholder(processed.content);
      }
      if (processed.metadata) {
        processed.metadata = "[hashed]";
      }
    } else if (level === "standard") {
      // Hash message content, keep metadata
      if (processed.content) {
        processed.content = this._hashPlaceholder(processed.content);
      }
    }

    processed.privacy_level = level;
    return processed;
  }

  /**
   * Store an ITP event locally in IndexedDB (fallback when native host unavailable).
   */
  async storeLocally(event) {
    const db = await this._getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, "readwrite");
      tx.objectStore(STORE_NAME).add({
        ...event,
        stored_at: new Date().toISOString(),
      });
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  }

  /**
   * Retrieve and flush locally stored events (for batch upload when native host reconnects).
   */
  async flushLocal(batchSize = 100) {
    const db = await this._getDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, "readwrite");
      const store = tx.objectStore(STORE_NAME);
      const events = [];
      const req = store.openCursor();
      req.onsuccess = (e) => {
        const cursor = e.target.result;
        if (cursor && events.length < batchSize) {
          events.push(cursor.value);
          cursor.delete();
          cursor.continue();
        } else {
          resolve(events);
        }
      };
      req.onerror = () => reject(req.error);
    });
  }

  _hashPlaceholder(content) {
    // In production, use SubtleCrypto SHA-256.
    // For now, return a placeholder indicating content was hashed.
    return `[sha256:${content.length}]`;
  }

  async _getDB() {
    if (this._db) return this._db;
    return new Promise((resolve, reject) => {
      const req = indexedDB.open(DB_NAME, DB_VERSION);
      req.onupgradeneeded = () => {
        const db = req.result;
        if (!db.objectStoreNames.contains(STORE_NAME)) {
          db.createObjectStore(STORE_NAME, { autoIncrement: true });
        }
      };
      req.onsuccess = () => {
        this._db = req.result;
        resolve(this._db);
      };
      req.onerror = () => reject(req.error);
    });
  }
}
