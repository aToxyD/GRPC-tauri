<script lang="ts">
  import { notifications, removeNotification } from '../lib/notifications';
  import type { Notification } from '../lib/types';

  function getIcon(type: Notification['type']) {
    switch (type) {
      case 'success':
        return 'M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z';
      case 'error':
        return 'M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z';
      case 'warning':
        return 'M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z';
      case 'info':
        return 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z';
    }
  }

  function getColorClasses(type: Notification['type']) {
    switch (type) {
      case 'success':
        return 'bg-green-50 border-green-200 text-green-800';
      case 'error':
        return 'bg-red-50 border-red-200 text-red-800';
      case 'warning':
        return 'bg-yellow-50 border-yellow-200 text-yellow-800';
      case 'info':
        return 'bg-blue-50 border-blue-200 text-blue-800';
    }
  }

  function getIconColor(type: Notification['type']) {
    switch (type) {
      case 'success':
        return 'text-green-600';
      case 'error':
        return 'text-red-600';
      case 'warning':
        return 'text-yellow-600';
      case 'info':
        return 'text-blue-600';
    }
  }

  function formatTime(timestamp: string) {
    const date = new Date(timestamp);
    return date.toLocaleTimeString('ar-DZ', { 
      hour: '2-digit', 
      minute: '2-digit' 
    });
  }

  $: notificationList = $notifications.slice(0, 5); // Show max 5 notifications
</script>

<div class="fixed top-4 left-4 z-50 space-y-2 max-w-sm">
  {#each notificationList as notification (notification.id)}
    <div 
      class="notification-slide border rounded-lg p-4 shadow-lg animate-slide-in"
      class:bg-green-50={notification.type === 'success'}
      class:bg-red-50={notification.type === 'error'}
      class:bg-yellow-50={notification.type === 'warning'}
      class:bg-blue-50={notification.type === 'info'}
      class:border-green-200={notification.type === 'success'}
      class:border-red-200={notification.type === 'error'}
      class:border-yellow-200={notification.type === 'warning'}
      class:border-blue-200={notification.type === 'info'}
    >
      <div class="flex items-start gap-3">
        <!-- Icon -->
        <div class="flex-shrink-0 mt-0.5">
          <svg 
            class="w-5 h-5 {getIconColor(notification.type)}"
            fill="none" 
            stroke="currentColor" 
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="{getIcon(notification.type)}"></path>
          </svg>
        </div>

        <!-- Content -->
        <div class="flex-1 min-w-0">
          <h4 class="font-medium text-sm text-gray-900 dark:text-white">
            {notification.title}
          </h4>
          <p class="text-sm text-gray-700 dark:text-gray-300 mt-1">
            {notification.message}
          </p>
          <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">
            {formatTime(notification.timestamp)}
          </p>
        </div>

        <!-- Close button -->
        <button
          on:click={() => removeNotification(notification.id)}
          class="flex-shrink-0 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
          aria-label="إغلاق الإشعار"
          title="إغلاق"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
          </svg>
        </button>
      </div>
    </div>
  {/each}

  {#if $notifications.length > 5}
    <div class="text-center text-xs text-gray-500 dark:text-gray-400 py-2">
      +{$notifications.length - 5} إشعارات أخرى
    </div>
  {/if}
</div>

<style>
  .notification-slide {
    animation: slideIn 0.3s ease-out;
  }

  @keyframes slideIn {
    from {
      transform: translateX(-100%);
      opacity: 0;
    }
    to {
      transform: translateX(0);
      opacity: 1;
    }
  }

  .animate-slide-out {
    animation: slideOut 0.3s ease-in forwards;
  }

  @keyframes slideOut {
    from {
      transform: translateX(0);
      opacity: 1;
    }
    to {
      transform: translateX(-100%);
      opacity: 0;
    }
  }
</style>
